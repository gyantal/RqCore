use std::{sync::{Arc, LazyLock, Mutex}, env, time::Instant};
use ibapi::prelude::*;

use rqcommon::{log_and_println, utils::server_ip::ServerIp};

use crate::gateway::Gateway;

// ---------- Global static variables ----------
pub static RQ_BROKERS_WATCHER: LazyLock<BrokersWatcher> = LazyLock::new(|| BrokersWatcher::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RqOrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct RqOrder {
    pub order_type: RqOrderType,
    pub ticker: String,
    pub company_name: String,
    pub pos_market_value: f64,
    pub known_last_price: Option<f64>,
}

// ---------- BrokersWatcher ----------
pub struct BrokersWatcher {
    // TODO: use Arc<Mutex<BrokersWatcher>>
    // This Mutex will assures that only 1 thread can access the BrokerWatcher, which is too much restriction,
    // because 1. it can be multithreaded, or that if it contains 2 clients, those 2 clients should be accessed parallel.
    // However, it will suffice for a while. Yes. We will need the mutex at lower level later.

    // gateways are Arc<Mutex<Gateway>> so each gateway can be locked independently
    pub gateways: Mutex<Vec<Arc<Mutex<Gateway>>>>,
}

impl BrokersWatcher {
    pub fn new() -> Self {
        BrokersWatcher { gateways: Mutex::new(Vec::new()) }
    }

    pub fn gateway_client_id() -> i32 {
        if env::consts::OS == "windows" {
            // On windows, use USERDOMAIN, instead of USERNAME, because USERNAME can be the same on multiple machines (e.g. "gyantal" on both GYANTAL-PC and GYANTAL-LAPTOP)
            let userdomain = env::var("USERDOMAIN").expect("Failed to get USERDOMAIN environment variable");
            match userdomain.as_str() {
                "GYANTAL-PC" => 210,
                "GYANTAL-LAPTOP" => 211,
                "BALAZS-PC" => 212,
                "BALAZS-LAPTOP" => 213,
                "DAYA-DESKTOP" => 214,
                "DAYA-LAPTOP" => 215,
                "DRCHARMAT-LAPTOP" => 216,
                _ => panic!(
                    "Windows user name is not recognized. Add your username and folder here!"
                ),
            }
        } else { // Linux and MacOS
            200
        }
    }

    pub async fn init(&self) {
        log::info!("BrokersWatcher.init() start");
        // Initialize all gateways
        let client_id = Self::gateway_client_id();
        let mut gateways = self.gateways.lock().unwrap();

        let connection_url_dcmain = [ServerIp::sq_core_server_public_ip_for_clients(), ":", ServerIp::IB_SERVER_PORT_DCMAIN.to_string().as_str()].concat();
        let mut gateway0 = Gateway::new(&connection_url_dcmain, client_id);
        gateway0.init().await;
        gateways.push(Arc::new(Mutex::new(gateway0)));

        let connection_url_gyantal = [ServerIp::sq_core_server_public_ip_for_clients(), ":", ServerIp::IB_SERVER_PORT_GYANTAL.to_string().as_str()].concat();
        let mut gateway1 = Gateway::new(&connection_url_gyantal, client_id);
        gateway1.init().await;
        gateways.push(Arc::new(Mutex::new(gateway1)));
    }

    pub async fn exit(&self) {
        let mut gateways = self.gateways.lock().unwrap();
        for gateway in gateways.iter() {
            gateway.lock().unwrap().exit().await;
        }
        gateways.clear();
    }

    // TODO: future features.
    // BrokersWatches should access a global YahooFinance price cache, and if the price is fresh (e.g. within 1 min), then it can use that price instead of calling IB get_price() which can be slow (e.g. 1-2 seconds). 
    // This will speed up the order placing a lot, because we can avoid calling IB get_price() [550ms] for every order.
    pub async fn place_orders(&self, orders: Vec<RqOrder>, is_simulation: bool) {
        if orders.is_empty() {
            log_and_println!("BrokersWatcher.place_orders(): no orders.");
            return;
        }

        // Acquire and clone the ib_client handles (Arc<Client>) without holding locks across await
        let ib_client_gyantal = { // 0 is dcmain, 1 is gyantal
            let gateways = self.gateways.lock().unwrap();
            gateways[1].lock().unwrap().ib_client.as_ref().cloned().expect("ib_client is not initialized")
        };
        let ib_client_dcmain = { // 0 is dcmain, 1 is gyantal
            let gateways = self.gateways.lock().unwrap();
            gateways[0].lock().unwrap().ib_client.as_ref().cloned().expect("ib_client is not initialized")
        };

        // This will do a real trade. To prevent trade happening you have 3 options.
        // 1. Comment out ib_client.order() (for both Buy/Sell) Just comment it back in when you want to trade.
        // 2. Another option to prevent trade: is_simulation bool.
        // 3. Another option to prevent trade: in IbGateway settings, check in "ReadOnly API".
        log_and_println!("BrokersWatcher.place_orders(): {} order(s). Simulation: {}", orders.len(), is_simulation);

        // TODO: if YahooFinance price cache is implemented. 2 loops are needed for fast execution. First loop gets prices from YF cache.
        // If price is not found in YF cache, it puts those orders in a 'unknown_price_orders' list to be processed in the second loop, which calls IB get_price() taking 550ms per order.
        for order in &orders {
            let known_price_str = order.known_last_price.map(|p| format!("{:.4}", p)).unwrap_or_else(|| "N/A".to_string());
            log_and_println!("  {:?} {} ({}, known_last_price: ${}, target posValue: ${}, before get_price())", order.order_type, order.ticker, order.company_name, known_price_str, order.pos_market_value);

            let contract = Contract::stock(&order.ticker).build();
            let price = Self::get_price(&ib_client_dcmain, &order.ticker, &order.company_name, order.known_last_price, &contract).await;
            if price.is_nan() {
                log_and_println!("  {:?} {} ({}, cannot determine price, skipping...)", order.order_type, order.ticker, order.company_name);
                continue;
            }

            let num_shares = (order.pos_market_value / price).floor() as i32;
            if num_shares <= 0 {
                log_and_println!("  {:?} {} ({}, price: ${}, nShares: {}, skipping...)", order.order_type, order.ticker, order.company_name, price, num_shares);
                continue;
            }

            log_and_println!("  {:?} {} ({}, price: ${}, nShares: {}, before order())", order.order_type, order.ticker, order.company_name, price, num_shares);

            if is_simulation {
                continue;
            }

            let order_id = match order.order_type {
                RqOrderType::Buy => {
                    ib_client_gyantal
                        .order(&contract)
                        .buy(num_shares)
                        // .market()
                        // Limit buy order at 2.1% above the price. IB rejects too-wide LMT orders.
                        .limit(((price * 1.021) * 100.0).round() / 100.0)
                        .submit()
                        .await
                        .expect("order submission failed!")
                }
                RqOrderType::Sell => {
                    ib_client_gyantal
                        .order(&contract)
                        .sell(num_shares)
                        // .market()
                        // Limit sell order at -2.1% below price
                        .limit(((price * 0.979) * 100.0).round() / 100.0)
                        .submit()
                        .await
                        .expect("order submission failed!")
                }
            };

            log_and_println!("Order submitted: OrderID: {}, Ticker: {}, Shares: {}", order_id, contract.symbol, num_shares);
        }
    }

    pub async fn get_price(ib_client_dcmain: &Arc<Client>, ticker: &str, company_name: &str, known_last_price: Option<f64>, contract: &Contract) -> f64 {
        if let Some(price) = known_last_price {
            if !price.is_nan() {
                return price;
            }
        }

        // TODO: there is probably a better way to get the last price, e.g. via market data snapshot than streaming real-time bars.
        // Also, it only works during market hours. After market closes, it doesn't return any bars and we wait forever.
        // We ask the 5 seconds bars, but luckily the first bar comes immediately. Later new bars arrive every 5 seconds.
        let start = Instant::now();
        let mut subscription = ib_client_dcmain
            .realtime_bars(contract, RealtimeBarSize::Sec5, RealtimeWhatToShow::Trades, TradingHours::Regular)
            .await
            .expect("realtime bars request failed!");

        log_and_println!("  {} ({}, waiting for real-time bar...)", ticker, company_name);

        let mut price = f64::NAN;
        while let Some(bar_result) = subscription.next().await {
            match bar_result {
                Ok(bar) => {
                    log_and_println!("  {bar:?}");
                    price = bar.close;
                }
                Err(e) => log::error!("Error in realtime_bars subscription: {e:?}"),
            }
            let elapsed_microsec = start.elapsed().as_secs_f64() * 1_000_000.0;
            log_and_println!("  Elapsed Time of ib_client.realtime_bars(): {:.2}us", elapsed_microsec);
            break; // just 1 bar
        }

        price
    }
}
