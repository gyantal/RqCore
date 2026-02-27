use std::{collections::HashMap, env, fmt, fmt::Write, sync::{Arc, LazyLock, Mutex}, time::Instant};
use chrono::Utc;
use ibapi::prelude::*;
use ibapi::orders::{CommissionReport, ExecutionData, ExecutionFilter, Executions};

use rqcommon::{log_and_println, rqhelper::MutexExt, utils::server_ip::ServerIp};
use memdb::mark_value_cache::RQ_MARK_VALUE_CACHE;

use crate::gateway::Gateway;

// ---------- Global static variables ----------
pub static RQ_BROKERS_WATCHER: LazyLock<BrokersWatcher> = LazyLock::new(|| BrokersWatcher::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrokerClient { // IbClient or TsClient
    DcMain,
    DcBlanzac,
    Gyantal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RqOrderType {
    Buy,
    Sell,
}

impl fmt::Display for RqOrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RqOrderType::Buy => write!(f, "BUY"),
            RqOrderType::Sell => write!(f, "SELL"),
        }
    }
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

    pub gateways: Mutex<HashMap<BrokerClient, Gateway>>,
}

impl BrokersWatcher {
    pub fn new() -> Self {
        BrokersWatcher { gateways: Mutex::new(HashMap::new()) }
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

        let mut mark_value_cache = RQ_MARK_VALUE_CACHE.lock_ignore_poison();
        mark_value_cache.init();

        // Initialize all gateways
        let client_id = Self::gateway_client_id();
        let mut gateways = self.gateways.lock_ignore_poison();

        let connection_url_dcmain = [ServerIp::sq_core_server_public_ip_for_clients(), ":", ServerIp::IB_SERVER_PORT_DCMAIN.to_string().as_str()].concat();
        let mut gateway_dcmain = Gateway::new(&connection_url_dcmain, client_id);
        gateway_dcmain.init().await;
        gateways.insert(BrokerClient::DcMain, gateway_dcmain);

        let connection_url_dcblanzac = [ServerIp::sq_core_server_public_ip_for_clients(), ":", ServerIp::IB_SERVER_PORT_DCBLANZAC.to_string().as_str()].concat();
        let mut gateway_dcblanzac = Gateway::new(&connection_url_dcblanzac, client_id);
        gateway_dcblanzac.init().await;
        gateways.insert(BrokerClient::DcBlanzac, gateway_dcblanzac);

        let connection_url_gyantal = [ServerIp::sq_core_server_public_ip_for_clients(), ":", ServerIp::IB_SERVER_PORT_GYANTAL.to_string().as_str()].concat();
        let mut gateway_gyantal = Gateway::new(&connection_url_gyantal, client_id);
        gateway_gyantal.init().await;
        gateways.insert(BrokerClient::Gyantal, gateway_gyantal);
    }

    pub async fn exit(&self) {
        let mut gateways = self.gateways.lock_ignore_poison();
        for gateway in gateways.values_mut() {
            gateway.exit().await;
        }
        gateways.clear();
    }

    pub async fn get_order_executions(&self, broker_client: BrokerClient) -> (Vec<ExecutionData>, Vec<CommissionReport>) {
        let ib_client = {
            let gateways = self.gateways.lock_ignore_poison();
            let Some(gateway) = gateways.get(&broker_client) else {
                log::error!("BrokersWatcher.get_order_executions(): gateway is missing for {:?}.", broker_client);
                return (Vec::new(), Vec::new());
            };
            let Some(client) = gateway.ib_client.as_ref().cloned() else {
                log::error!("BrokersWatcher.get_order_executions(): ib_client is not initialized for {:?}.", broker_client);
                return (Vec::new(), Vec::new());
            };
            client
        };

        // IbGateway: only gives today's executions (since midnight). No matter about the filter. 
        // TODO: I have to test that it works for client_gyantal
        // IbTWS: by default only today's executions. But if a user changes it to 7 days inside TWS, then it gives that.
        let mut subscription = match ib_client.executions(ExecutionFilter::default()).await {
            Ok(subscription) => subscription,
            Err(e) => {
                log::error!("BrokersWatcher.get_order_executions(): failed to request executions for {:?}: {:?}", broker_client, e);
                return (Vec::new(), Vec::new());
            }
        };

        let mut execution_data = Vec::new();
        let mut commission_reports = Vec::new();

        while let Some(result) = subscription.next().await {
            match result {
                Ok(Executions::ExecutionData(data)) => execution_data.push(data),
                Ok(Executions::CommissionReport(report)) => commission_reports.push(report),
                Ok(Executions::Notice(_)) => {}
                Err(ibapi::Error::EndOfStream) => break,
                Err(e) => {
                    log::error!("BrokersWatcher.get_order_executions(): unexpected stream error for {:?}: {:?}", broker_client, e);
                    break;
                }
            }
        }

        (execution_data, commission_reports)
    }

    // TODO: future features.
    // BrokersWatches should access a global YahooFinance price cache, and if the price is fresh (e.g. within 1 min), then it can use that price instead of calling IB get_price() which can be slow (e.g. 1-2 seconds). 
    // This will speed up the order placing a lot, because we can avoid calling IB get_price() [550ms] for every order.
    pub async fn place_orders(&self, orders: Vec<RqOrder>, is_simulation: bool, user_log: &mut String) {
        if orders.is_empty() {
            log_and_println!("BrokersWatcher.place_orders(): no orders.");
            return;
        }

        log_and_println!("BrokersWatcher.place_orders(): {} order(s). Simulation: {}", orders.len(), is_simulation);

        let mut ticker_markvalues: HashMap<String, f64> = HashMap::new(); // This HashMap will not contain NaN. If it is NaN, we don't put in.
        let now = Utc::now();
        { // Scope Mutex.lock() to avoid holding them for more than necessary. Good practice.
            let mark_value_cache = RQ_MARK_VALUE_CACHE.lock_ignore_poison();
            for (ticker, mark_value, mark_time) in mark_value_cache.get_mark_timevalues(orders.iter().map(|order| order.ticker.as_str()))
            {
                log_and_println!("  MarkValue cache: {} => value: {}, time: {}", ticker, mark_value, mark_time);
                writeln!(user_log, "  MarkValue cache: {} => value: {}, time: {}", ticker, mark_value, mark_time).ok();
                if mark_time < now - chrono::Duration::minutes(2) {
                    log_and_println!("  MarkValue cache: {} is stale (value: {}, time: {}). Consider improving the cache freshness or reliability.", ticker, mark_value, mark_time);
                }
                if !mark_value.is_nan() {
                    ticker_markvalues.insert(ticker.to_string(), mark_value);
                }
            }
        }

        // Acquire and clone the ib_client handles (Arc<Client>) without holding locks across await
        let ib_client_gyantal = {
            let gateways = self.gateways.lock_ignore_poison();
            let Some(gateway) = gateways.get(&BrokerClient::Gyantal) else {
                log_and_println!("BrokersWatcher.place_orders(): gyantal gateway is missing.");
                return;
            };
            let Some(client) = gateway.ib_client.as_ref().cloned() else {
                log_and_println!("BrokersWatcher.place_orders(): gyantal ib_client is not initialized.");
                return;
            };
            client
        };
        let ib_client_dcmain = {
            let gateways = self.gateways.lock_ignore_poison();
            let Some(gateway) = gateways.get(&BrokerClient::DcMain) else {
                log_and_println!("BrokersWatcher.place_orders(): dcmain gateway is missing.");
                return;
            };
            let Some(client) = gateway.ib_client.as_ref().cloned() else {
                log_and_println!("BrokersWatcher.place_orders(): dcmain ib_client is not initialized.");
                return;
            };
            client
        };

        // 2 loops are needed for fast execution. First loop gets prices from YF cache and trades immediately.
        // If price is not found in YF cache, it puts those orders in a 'unknown_price_orders' list to be processed in the second loop, which calls IB get_price() taking 550ms per order.
        let mut unknown_price_orders: Vec<&RqOrder> = Vec::new();
        for order in &orders {
            if let Some(price) = ticker_markvalues.get(&order.ticker) { // if price is found in ticker_markvalues, then use it.
                log_and_println!("  Using MarkValue cache price for {}: ${}", order.ticker, price);
                BrokersWatcher::place_order(is_simulation, &ib_client_gyantal, &ib_client_dcmain, order, *price, user_log).await;
            } else {
                log_and_println!("  No valid MarkValue cache price for {}. Will call IB get_price() which can be slow (e.g. 550ms)...", order.ticker);
                unknown_price_orders.push(order);
            }
        }

        for order in &unknown_price_orders {
            let price : f64 = BrokersWatcher::get_knownlast_or_ib_price(&ib_client_dcmain, &order.ticker, &order.company_name, order.known_last_price).await;
            log_and_println!("  IB get_price() for {}: ${}", order.ticker, price);
            BrokersWatcher::place_order(is_simulation, &ib_client_gyantal, &ib_client_dcmain, order, price, user_log).await;
        }

    }

    async fn place_order(is_simulation: bool, ib_client_gyantal: &Arc<Client>, _ib_client_dcmain: &Arc<Client>, order: &RqOrder, price : f64, user_log: &mut String) {
        if price.is_nan() {
            log_and_println!("  {:?} {} ({}, cannot determine price, skipping...)", order.order_type, order.ticker, order.company_name);
            writeln!(user_log, "  {:?} {} ({}, cannot determine price, skipping...)", order.order_type, order.ticker, order.company_name).ok();
            return;
        }
        let num_shares = (order.pos_market_value / price).floor() as i32;
        if num_shares <= 0 {
            log_and_println!("  {:?} {} ({}, price: ${}, nShares: {}, zero size, skipping...)", order.order_type, order.ticker, order.company_name, price, num_shares);
            writeln!(user_log, "  {:?} {} ({}, price: ${}, nShares: {}, zero size, skipping...)", order.order_type, order.ticker, order.company_name, price, num_shares).ok();
            return;
        }
        log_and_println!("  {:?} {} ({}, price: ${}, nShares: {}, before order())", order.order_type, order.ticker, order.company_name, price, num_shares);
        writeln!(user_log, "  {:?} {} ({}, price: ${}, nShares: {}, before order())", order.order_type, order.ticker, order.company_name, price, num_shares).ok();
        if is_simulation {
            return;
        }

        // This will do a real trade. To prevent trade happening you have 3 options.
        // 1. Comment out ib_client.order() (for both Buy/Sell) Just comment it back in when you want to trade.
        // 2. Another option to prevent trade: is_simulation bool.
        // 3. Another option to prevent trade: in IbGateway settings, check in "ReadOnly API".

        let contract = Contract::stock(&order.ticker).build();
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

    pub async fn get_knownlast_or_ib_price(ib_client_dcmain: &Arc<Client>, ticker: &str, company_name: &str, known_last_price: Option<f64>) -> f64 {
        if let Some(price) = known_last_price {
            if !price.is_nan() {
                return price;
            }
        }

        let contract = Contract::stock(ticker).build();
        // TODO: there is probably a better way to get the last price, e.g. via market data snapshot than streaming real-time bars.
        // Also, it only works during market hours. After market closes, it doesn't return any bars and we wait forever.
        // We ask the 5 seconds bars, but luckily the first bar comes immediately. Later new bars arrive every 5 seconds.
        let start = Instant::now();
        let mut subscription = ib_client_dcmain
            .realtime_bars(&contract, RealtimeBarSize::Sec5, RealtimeWhatToShow::Trades, TradingHours::Regular)
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