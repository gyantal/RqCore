use std::path::{Path, PathBuf} ;
use std::collections::HashMap;
use std::{thread};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex, MutexGuard};
use actix_web::{rt::System};
use chrono::{Local, Utc};
use chrono::Datelike;
use serde::Deserialize;
use ibapi::{prelude::*};
use std::fs;
use std::time::SystemTime;
use std::future::Future;

use crate::RQ_BROKERS_WATCHER;

use std::time::Instant;

pub fn benchmark_elapsed_time(name: &str, f: impl FnOnce()) {
    let start = Instant::now();
    f();
    let elapsed = start.elapsed();
    let micros = elapsed.as_secs_f64() * 1_000_000.0;
    println!("Elapsed Time of {}: {:.2}us", name, micros); // TODO: no native support thousand separators in float or int. Use crate 'num-format' or 'thousands' or better: write a lightweight formatter train in RqCommon
}

pub async fn benchmark_elapsed_time_async<F, Fut>(name: &str, f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let start = Instant::now();
    f().await;
    let elapsed = start.elapsed();
    let micros = elapsed.as_secs_f64() * 1_000_000.0;
    println!("Elapsed Time of {}: {:.2}us", name, micros);
}

#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    pub data: Vec<Transaction>,
    pub included: Vec<Stock>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Transaction {
    pub id: String,  // in JSON, it is a String
    #[serde(rename = "type")]
    pub type_: String,
    pub attributes: TransactionAttributes,
    pub relationships: TransactionRelationships,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum Weight {
    Number(f64),
    String(String),
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TransactionAttributes {
    pub id: i64,
    pub action: String,
    #[serde(rename = "actionDate")]
    pub action_date: String,
    #[serde(rename = "startingWeight")]
    pub starting_weight: Option<Weight>,
    #[serde(rename = "newWeight")]
    pub new_weight: Option<Weight>,
    pub rule: Option<String>,
    pub status: Option<String>, // only appears in JSON, when stock market is closed. e.g. "status": "closed",
    pub price: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionRelationships {
    pub ticker: TickerRelationship,
}

#[derive(Debug, Deserialize)]
pub struct TickerRelationship {
    pub data: TickerData,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TickerData {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
}



#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct Stock {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub attributes: StockAttributes,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct StockAttributes {
    pub name: String,
    #[serde(rename = "companyName")]
    pub company_name: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TransactionEvent {
    pub transaction_id: String,
	pub action_date: String,
	pub ticker: String, // this is StockAttributes.name
	pub company_name: String,
	pub starting_weight: Option<Weight>,
	pub new_weight: Option<Weight>,
	pub price: Option<String>,
    pub pos_weight: f32, // calculated position weight in percentage (0.0 to 100.0)
    pub pos_market_value: f64, // calculated position market value in USD
}


pub struct FastRunner {
    pub is_simulation: bool, // is true for simulation, false for real trading
    pub loop_sleep_ms_simulation: u64,
    pub loop_sleep_ms_realtrading: u64,
    pub is_loop_active: Arc<AtomicBool>,
    pub has_trading_ever_started: bool,
    pub cookies: Option<String>,
    pub cookies_file_last_modtime: Option<SystemTime>,
}

impl FastRunner {
    // When to schedule the service?
    // Until 2025-10-17: History page was updated at 9:30:00 AM ET on Mondays (after market open). Exactly at that second when market opens.
    // On 2025-10-27: Analysis page was updated 12:00:00 PM ET, but History page was only updated later, at 12:02:00 PM ET. So, run the loop from T-30sec to T+3min. Or implement reading Analysis page too.
    pub fn new() -> Self {
        Self {
            is_simulation: true, // at trading, change this to false. Also check if IbGateway is in ReadOnly mode.

            loop_sleep_ms_simulation: 3750, // usually 3750
            loop_sleep_ms_realtrading: 500, // usually 500ms (note that reqwest.client.get() is 500-700ms)
            is_loop_active: Arc::new(AtomicBool::new(false)),
            has_trading_ever_started: false,
            // initialize cookies cache
            cookies: None,
            cookies_file_last_modtime: None,
        }
    }

    const COOKIES_FILE_PATH: &'static str = "../../../rqcore_data/fast_run_1_headers.txt";
    fn ensure_cookies_loaded(&mut self) {
        let file_metadata = fs::metadata(Self::COOKIES_FILE_PATH).expect("metadata() failed for cookies file");
        let file_modified_time = file_metadata.modified().expect("modified() failed for cookies file");

        let need_reload = self.cookies.is_none()
            || self.cookies_file_last_modtime.map(|t| t != file_modified_time).unwrap_or(true);

        if need_reload {
            let content = fs::read_to_string(Self::COOKIES_FILE_PATH).expect("read_to_string() failed!");
            self.cookies = Some(content.trim().to_string());
            self.cookies_file_last_modtime = Some(file_modified_time);
            println!("Cookies loaded/refreshed from file.");
        }
    }

    pub async fn get_new_buys_sells(&mut self) -> (String, Vec<TransactionEvent>, Vec<TransactionEvent>) {
        println!(">* get_new_buys_sells() started.");

        // Calculate target_action_date from the current date (last Monday). TODO: This can be a problem if Monday is a stock market holiday. We will miss that date. But OK for now.
        let current_date = Utc::now().date_naive();
        let days_to_subtract = current_date.weekday().num_days_from_monday() as i64;
        let target_action_date_naive = current_date - chrono::Duration::days(days_to_subtract);
        let target_action_date = target_action_date_naive.format("%Y-%m-%d").to_string();
        // target_action_date = "2025-10-17".to_string(); // Monday date

        benchmark_elapsed_time("ensure_cookies_loaded()", || {  // 300us first, 70us later
            self.ensure_cookies_loaded(); // cookies are reloaded from file only if needed, if the file changed.
        });

        const URL: &str = "https://seekingalpha.com/api/v3/quant_pro_portfolio/transactions?include=ticker.slug%2Cticker.name%2Cticker.companyName&page[size]=1000&page[number]=1";

        let mut body_text: String= String::new();
        benchmark_elapsed_time_async("reqwest.Client.get()", || async { // 1,800-3,600ms first, 500-700ms later with keep-alive
            // Build client with cookies
            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build().expect("Client::builder() failed!");

            let resp = client.get(URL)
                .header("Cookie", self.cookies.as_deref().expect("cookies not initialized"))
                .send()
                .await
                .expect("reqwest client.get() failed!");

            // Get response as text first
            body_text = resp.text().await.expect("resp.text() failed!");
        }).await;
        
        if body_text.len() < 1000 {
            if body_text.contains("Subscription is required") {
                println!("No permission, Update cookie file."); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
                return (target_action_date.to_string(), Vec::new(), Vec::new());
            } else if body_text.contains("captcha.js") {
                println!("Captcha required, Update cookie file AND handle Captcha in browser."); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
                return (target_action_date.to_string(), Vec::new(), Vec::new());
            }
        }
        
        // Save raw response
        let dir = Path::new("../../../rqcore_data");
        tokio::fs::create_dir_all(dir).await.expect("create_dir_all() failed!"); // create folder if not exists
        let datetime_str = Local::now().format("%Y%m%dT%H%M%S").to_string();
        let file_path: PathBuf = dir.join(format!("fast_run_1_src_{}.json", datetime_str));
        tokio::fs::write(&file_path, &body_text).await.expect("fs::write() failed!");
        println!("Saved raw JSON to {}", file_path.display());
        
        // Parse saved text as JSON
        let api_response: ApiResponse = serde_json::from_str(&body_text).expect("serde_json::from_str() failed!");
        
        // Extract transactions list (Vec<Transaction>)
        let transactions = api_response.data;
        
        // Extract stocks dictionary (HashMap<String, Stock>)
        let mut stocks: HashMap<String, Stock> = HashMap::new();
        for stock in api_response.included {
            stocks.insert(stock.id.clone(), stock);
        }

        println!("Found {} transactions, {} stocks", transactions.len(), stocks.len());
        // Print first transaction example
        // if let Some(first_tx) = transactions.first() {
        //     println!("First transaction: id={}, action={}, price={:?}, tickerId={}", 
        //         first_tx.attributes.id, 
        //         first_tx.attributes.action, 
        //         first_tx.attributes.price,
        //         first_tx.relationships.ticker.data.id
        //     );
        // }

        // Print all transactions
        // for transaction in &transactions {
        //     let ticker_id = &transaction.relationships.ticker.data.id;
        //     if let Some(stock) = stocks.get(ticker_id) {
        //         println!(
        //             "Tr {}: {} {} {} weight of {} ({}) at ${}",
        //             transaction.attributes.id,
        //             transaction.attributes.actionDate,
        //             transaction.attributes.action,
        //             transaction.attributes.new_weight,
        //             stock.attributes.name,
        //             stock.attributes.companyName,
        //             transaction.attributes.price.as_deref().unwrap_or("N/A"),
        //         );
        //     }
        // }

        // Collect buys/sells for the specific date
        let mut new_buy_events: Vec<TransactionEvent> = Vec::new();
        let mut new_sell_events: Vec<TransactionEvent> = Vec::new();
        for transaction in &transactions {
            // Skip if not our target date
            if transaction.attributes.action_date != target_action_date {
                continue;
            }
            // Skip rebalance transactions
            if transaction.attributes.rule.as_deref() == Some("rebalance") {
                continue;
            }
            let stock = stocks.get(&transaction.relationships.ticker.data.id);
            match transaction.attributes.action.as_str() {
                "buy" => if let Some(stock) = stock {
                    new_buy_events.push(TransactionEvent {
                        transaction_id: transaction.id.clone(),
                        action_date: transaction.attributes.action_date.clone(),
                        ticker: stock.attributes.name.clone(),
                        company_name: stock.attributes.company_name.clone(),
                        starting_weight: transaction.attributes.starting_weight.clone(),
                        new_weight: transaction.attributes.new_weight.clone(),
                        price: transaction.attributes.price.clone(),
                        pos_weight: 0.0,
                        pos_market_value: 0.0,
                    });
                },
                "sell" => if let Some(stock) = stock {
                    new_sell_events.push(TransactionEvent {
                        transaction_id: transaction.id.clone(),
                        action_date: transaction.attributes.action_date.clone(),
                        ticker: stock.attributes.name.clone(),
                        company_name: stock.attributes.company_name.clone(),
                        starting_weight: transaction.attributes.starting_weight.clone(),
                        new_weight: transaction.attributes.new_weight.clone(),
                        price: transaction.attributes.price.clone(),
                        pos_weight: 0.0,
                        pos_market_value: 0.0,
                    });
                },
                _ => {}
            }
        }

        (target_action_date.to_string(), new_buy_events, new_sell_events)
    }

    fn determine_position_market_values_gyantal(&self, new_buy_events: &mut Vec<TransactionEvent>, new_sell_events: &mut Vec<TransactionEvent>) {
        let buy_pv = 10000.0; // PV for buys
        let sell_pv = 5000.0; // PV for sells

        let buy_pos_mkt_value = buy_pv / (new_buy_events.len() as f64);
        let sell_pos_mkt_value = sell_pv / (new_sell_events.len() as f64);
        for event in new_buy_events.iter_mut() {
            event.pos_market_value = buy_pos_mkt_value;
        }
        for event in new_sell_events.iter_mut() {
            event.pos_market_value = sell_pos_mkt_value;
        }
    }

    pub async fn test_http_download(&mut self) {
        let (target_action_date, new_buy_events, new_sell_events) = self.get_new_buys_sells().await;

        // Print summary
        println!("On {}, new positions:", target_action_date);
        print!("New BUYS ({}):", new_buy_events.len());
        for event in &new_buy_events {
            print!("  {} ({}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));
        }
        
        print!("\nNew SELLS ({}):", new_sell_events.len());
        for event in &new_sell_events {
            print!("  {} ({}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));
        }
        println!(); // print newline for flushing the buffer. Otherwise the last line may not appear immediately.
        // io::stdout().flush().unwrap();  // Ensure immediate output, because it is annoying to wait for newline or buffer full
    }


    pub async fn fastrunning_loop_impl(&mut self) {

        let (target_action_date, mut new_buy_events, mut new_sell_events) = self.get_new_buys_sells().await;

        let num_new_events = new_buy_events.len() + new_sell_events.len();
        if num_new_events == 0 {
            println!("No new buy/sell events on {}. Skipping trading.", target_action_date);
            return;
        }
        if num_new_events > 12 { // The most it was 5+5 = 10 trades in the past.
            println!("Something is wrong. Don't expect more than 10 events. num_new_events: {}. Skipping trading.", num_new_events);
            return;
        }

        // If we are here, there are events to trade. Assure that we trade only once.
        if self.has_trading_ever_started { // Assure that Trading only happens once per FastRunner instance. To avoid trading it many times.
            println!("Trading already started. Skipping this iteration.");
            return;
        }
        self.has_trading_ever_started = true;

        self.determine_position_market_values_gyantal(&mut new_buy_events, &mut new_sell_events); // replace it to blukucz if needed

        let gateways = RQ_BROKERS_WATCHER.gateways.lock().unwrap();
        let ib_client = gateways[1].ib_client.as_ref().unwrap(); // 0 is dcmain, 1 is gyantal

        // This will do a real trade. To prevent trade happening you have 3 options.
        // 1. Comment out ib_client.order() (for both Buy/Sell) Just comment it back in when you want to trade.
        // 2. Another option to prevent trade:self.is_simulation bool is true by default.
        // 3.Another option to prevent trade: is in IbGateway settings, check in "ReadOnly API", that will prevent the trades.
        println!("Loop: On {}, new positions:", target_action_date);
        println!("Process New BUYS ({}):", new_buy_events.len());
        for event in &new_buy_events {
            println!("  {} ({}, ${}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"), event.pos_market_value);

            if event.price.is_none()
                { continue;}
            let price_str = event.price.as_ref().unwrap();
            if price_str.parse::<f64>().is_err()
                { continue;}
            let price = price_str.parse::<f64>().unwrap();
            let num_shares = (event.pos_market_value / price).floor() as i32;
            let contract = Contract::stock(&event.ticker).build();
            println!("  {} ({}, nShares: {}) , ", event.ticker, event.company_name, num_shares);

            if self.is_simulation // prevent trade in simulation mode
                { continue;}
            let order_id = ib_client.order(&contract)
                .buy(num_shares)
                // .market()
                .limit(price * 1.20) // Limit buy order at 20% above the last day close price
                .submit()
                .await
                .expect("order submission failed!");
            println!("Order submitted: OrderID: {}, Ticker: {}, Shares: {}", order_id, contract.symbol, num_shares);
        }
        
        println!("Process New SELLS ({}):", new_sell_events.len());
        for event in &new_sell_events {
            println!("  {} ({}, ${}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"), event.pos_market_value);

            if event.price.is_none()
                { continue;}
            let price_str = event.price.as_ref().unwrap();
            if price_str.parse::<f64>().is_err()
                { continue;}
            let price = price_str.parse::<f64>().unwrap();
            let num_shares = (event.pos_market_value / price).floor() as i32;
            let contract = Contract::stock(&event.ticker).build();
            println!("  {} ({}, nShares: {}) , ", event.ticker, event.company_name, num_shares);

            if self.is_simulation // prevent trade in simulation mode
                { continue;}
            let order_id = ib_client.order(&contract)
                .sell(num_shares)
                //.market()
                .limit(price * 0.85) // Limit sell order at -15% below the last day close price
                .submit()
                .await
                .expect("order submission failed!");
            println!("Order submitted: OrderID: {}, Ticker: {}, Shares: {}", order_id, contract.symbol, num_shares);
        }
        println!(); // print newline for flushing the buffer. Otherwise the last line may not appear immediately.
        // io::stdout().flush().unwrap();  // Ensure immediate output, because it is annoying to wait for newline or buffer full
    }

    pub async fn start_fastrunning_loop(&mut self) {
        println!("start_fastrunning_loop() started.");
        self.is_loop_active.store(true, Ordering::SeqCst);
        let is_loop_active_clone = self.is_loop_active.clone(); // Clone the Arc, not the AtomicBool

        // tried to use tokio::spawn or actix_web::rt::spawn to start a task on the ThreadPool, but had problems that they were not called, because
        // the current thread is the ConsoleMenu main thread, and I never return from this thread. It should have worked though.
        // After 6 hours, I gave up and spawn a new OS thread here. 
        // The good side is that this new OS thread can process CPU-bound tasks faster than waiting for the ThreadPool delegation
        thread::spawn(move || {
            println!("FastRunner thread started");
            let sys = System::new(); // actix_web::rt::System to be able to use async in this new OS thread
            sys.block_on(async {
                let mut fast_runner2 = FastRunner::new(); // fake another instance, because self cannot be used, because it will be out of scope after this function returns

                while is_loop_active_clone.load(Ordering::SeqCst) {
                    println!(">* Loop iteration");

                    fast_runner2.fastrunning_loop_impl().await;

                    if fast_runner2.has_trading_ever_started {
                        println!("Trading has started, exiting the loop.");
                        break;
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(if fast_runner2.is_simulation { fast_runner2.loop_sleep_ms_simulation } else { fast_runner2.loop_sleep_ms_realtrading })).await;
                }
                println!("FastRunner thread stopping");
            });
        });
    }

    pub async fn stop_fastrunning_loop(&mut self) {
        println!("stop_fastrunning_loop() started.");
        self.is_loop_active.store(false, Ordering::SeqCst);
    }

}
