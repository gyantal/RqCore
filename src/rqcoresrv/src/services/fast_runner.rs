use chrono::Local;
use std::path::PathBuf;
use std::path::Path;
use serde::Deserialize;
use std::collections::HashMap;
use std::{thread};
use actix_web::{rt::System};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex, MutexGuard};
use ibapi::{prelude::*};

use crate::broker_common::brokers_watcher;

#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    pub data: Vec<Transaction>,
    pub included: Vec<Stock>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Transaction {
    pub id: String,  // in JNOS, it is a String
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
    pub new_weight: String,
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
	pub new_weight: String,
	pub price: Option<String>,
}


pub struct FastRunner {
    pub is_loop_active: Arc<AtomicBool>,
    pub fastrunning_loop_sleep_duration_ms: u64,
    pub is_trading_allowed: bool,
    pub has_trading_ever_started: bool,
}

impl FastRunner {
    pub fn new() -> Self {
        Self {
            is_loop_active: Arc::new(AtomicBool::new(false)),
            fastrunning_loop_sleep_duration_ms: 3750, // at trading, change this to 500ms
            is_trading_allowed: false, // at trading, change this to true. Also check if IbGateway is in ReadOnly mode.
            has_trading_ever_started: false,
        }
    }

    pub async fn get_new_buys_sells(&mut self) -> (String, Vec<TransactionEvent>, Vec<TransactionEvent>) {
        println!(">* get_new_buys_sells() started.");

        // Load cookies from file
        let cookies = std::fs::read_to_string("../../../rqcore_data/fast_run_1_headers.txt").expect("read_to_string() failed!");
        
        const URL: &str = "https://seekingalpha.com/api/v3/quant_pro_portfolio/transactions?include=ticker.slug%2Cticker.name%2Cticker.companyName&page[size]=1000&page[number]=1";

        // Prepare file path with timestamp        // Build client with cookies
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build().expect("Client::builder() failed!");

        let resp = client.get(URL)
            .header("Cookie", cookies.trim())
            .send()
            .await
            .expect("reqwest client.get() failed!");

        // Get response as text first
        let body_text = resp.text().await.expect("resp.text() failed!");
        
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

        // Collect buys/sells for specific date
        // TODO: calculate target_action_date from the current date (last Friday)
        let target_action_date: &str = "2025-10-17"; // Friday date
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
                    });
                },
                _ => {}
            }
        }

        (target_action_date.to_string(), new_buy_events, new_sell_events)
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


    pub async fn fastrunning_loop_impl(&mut self, brokers_watcher: &MutexGuard<'_, brokers_watcher::BrokersWatcher> ) {

        let (target_action_date, new_buy_events, new_sell_events) = self.get_new_buys_sells().await;

        let num_new_events = new_buy_events.len() + new_sell_events.len();
        if num_new_events == 0 {
            println!("No new buy/sell events on {}. Skipping trading.", target_action_date);
            return;
        }

        // If we are here, there are events to trade. Assure that we trade only once.
        if !self.is_trading_allowed
            { return; }

        if self.has_trading_ever_started { // Assure that Trading only happens once per FastRunner instance. To avoid trading it many times.
            println!("Trading already started. Skipping this iteration.");
            return;
        }
        self.has_trading_ever_started = true;

        let ib_client = brokers_watcher.gateways[1].ib_client.as_ref().unwrap(); // 0 is dcmain, 1 is gyantal
        let conn_url = &(brokers_watcher.gateways[1].connection_url); // 0 is dcmain, 1 is gyantal
        println!("Loop iteration. connUrl={}", conn_url);
        let position_market_value = 5000.0; // Don't overdo. The most it was 5+5 = 10 trades in the past.
        println!("On {}, new positions:", target_action_date);

        // This will do a real trade. To prevent trade happening you have 3 options.
        // 1. Comment out ib_client.order() (for both Buy/Sell) Just comment it back in when you want to trade.
        // 2. Another option to prevent trade:self.is_trading_allowed bool is false by default.
        // 3.Another option to prevent trade: is in IbGateway settings, check in "ReadOnly API", that will prevent the trades.

        println!("Process New BUYS ({}):", new_buy_events.len());
        for event in &new_buy_events {
            println!("  {} ({}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));

            if event.price.is_none()
                { continue;}
            let price_str = event.price.as_ref().unwrap();
            if price_str.parse::<f64>().is_err()
                { continue;}
            let price = price_str.parse::<f64>().unwrap();
            let num_shares = (position_market_value / price).floor() as i32;
            let contract = Contract::stock(&event.ticker).build();
            println!("  {} ({}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));

            let order_id = ib_client.order(&contract)
                .buy(num_shares)
                .market()
                .submit()
                .await
                .expect("order submission failed!");
            println!("Order submitted: OrderID: {}, Ticker: {}, Shares: {}", order_id, contract.symbol, num_shares);
        }
        
        println!("Process New SELLS ({}):", new_sell_events.len());
        for event in &new_sell_events {
            println!("  {} ({}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));

            if event.price.is_none()
                { continue;}
            let price_str = event.price.as_ref().unwrap();
            if price_str.parse::<f64>().is_err()
                { continue;}
            let price = price_str.parse::<f64>().unwrap();
            let num_shares = (position_market_value / price).floor() as i32;
            let contract = Contract::stock(&event.ticker).build();
            println!("  {} ({}, ${}) , ", event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));

            let order_id = ib_client.order(&contract)
                .sell(num_shares)
                .market()
                .submit()
                .await
                .expect("order submission failed!");
            println!("Order submitted: OrderID: {}, Ticker: {}, Shares: {}", order_id, contract.symbol, num_shares);
        }
        println!(); // print newline for flushing the buffer. Otherwise the last line may not appear immediately.
        // io::stdout().flush().unwrap();  // Ensure immediate output, because it is annoying to wait for newline or buffer full
    }

    pub async fn start_fastrunning_loop(&mut self, brokers_watcher_guard: &Arc<Mutex<brokers_watcher::BrokersWatcher>>) {
        println!("start_fastrunning_loop() started.");
        self.is_loop_active.store(true, Ordering::SeqCst);
        let is_loop_active_clone = self.is_loop_active.clone(); // Clone the Arc, not the AtomicBool
        let brokers_watcher_guard_clone = brokers_watcher_guard.clone(); // Clone the Arc, not BrokersWatcher

        // tried to use tokio::spawn or actix_web::rt::spawn to start a task on the ThreadPool, but had problems that they were not called, because
        // the current thread is the ConsoleMenu main thread, and I never return from this thread. It should have worked though.
        // After 6 hours, I gave up and spawn a new OS thread here. 
        // The good side is that this new OS thread can process CPU-bound tasks faster than waiting for the ThreadPool delegation
        thread::spawn(move || {
            println!("FastRunner thread started");
            let sys = System::new(); // actix_web::rt::System to be able to use async in this new OS thread
            sys.block_on(async {
                // TODO: this will lock the brokers_watcher for the whole loop duration, which efficient, but it is bad, because other parts of the program cannot access it while this loop is running.
                let brokers_watcher = brokers_watcher_guard_clone.lock().unwrap();
                let mut fast_runner2 = FastRunner::new(); // fake another instance, because self cannot be used, because it will be out of scope after this function returns

                while is_loop_active_clone.load(Ordering::SeqCst) {
                    println!("Loop iteration");

                    fast_runner2.fastrunning_loop_impl(&brokers_watcher).await;

                    if fast_runner2.has_trading_ever_started {
                        println!("Trading has started, exiting the loop.");
                        break;
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(fast_runner2.fastrunning_loop_sleep_duration_ms)).await;
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
