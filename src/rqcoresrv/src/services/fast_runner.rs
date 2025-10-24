use chrono::Local;
use std::path::PathBuf;
use std::path::Path;
use serde::Deserialize;
use std::collections::HashMap;
use std::{thread};
use actix_web::{rt::System};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex, MutexGuard};
// use std::io::{self, Write};

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

#[derive(Debug, Deserialize)]
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


pub struct FastRunner {
    pub is_loop_active: Arc<AtomicBool>
}

impl FastRunner {
    pub fn new() -> Self {
        Self {
            is_loop_active: Arc::new(AtomicBool::new(false))
        }
    }

    pub async fn get_new_buys_sells(&mut self) -> (String, Vec<Stock>, Vec<Stock>) {
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
        let mut new_buy_tickers: Vec<Stock> = Vec::new();
        let mut new_sell_tickers: Vec<Stock> = Vec::new();

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
                    new_buy_tickers.push(stock.clone());    // we have to clone the stock as we will return it to the caller
                },
                "sell" => if let Some(stock) = stock {
                    new_sell_tickers.push(stock.clone());
                },
                _ => {}
            }
        }

        (target_action_date.to_string(), new_buy_tickers, new_sell_tickers)
    }

    pub async fn test_http_download(&mut self) {
        let (target_action_date, new_buy_tickers, new_sell_tickers) = self.get_new_buys_sells().await;

        // Print summary
        println!("On {}, new positions:", target_action_date);
        print!("New BUYS ({}):", new_buy_tickers.len());
        for stock in &new_buy_tickers {
            print!("  {} ({}), ", stock.attributes.name, stock.attributes.company_name);
        }
        
        print!("\nNew SELLS ({}):", new_sell_tickers.len());
        for stock in &new_sell_tickers {
            print!("  {} ({}), ", stock.attributes.name, stock.attributes.company_name);
        }
        println!(); // print newline for flushing the buffer. Otherwise the last line may not appear immediately.
        // io::stdout().flush().unwrap();  // Ensure immediate output, because it is annoying to wait for newline or buffer full
    }


    pub async fn fastrunning_loop_impl(&mut self, brokers_watcher: &MutexGuard<'_, brokers_watcher::BrokersWatcher> ) {
        // The Real implementation what is running in the loop.
        // This and test_http_download() would have a lot of duplicate code, so we can refactor the common parts.
        // return the 2 vectors of new buys and new sells.
        // test_http_download() calls that and prints it.
        // fastrunning_loop_impl() calls that and uses the vectors for trading via IB API.

        let (target_action_date, new_buy_tickers, new_sell_tickers) = self.get_new_buys_sells().await;

        // TEMP: Use the brokers inside the loop
        let _ib_client_dcmain = brokers_watcher.gateways[0].ib_client.as_ref().unwrap(); // 0 is dcmain, 1 is gyantal
        let conn_url = &(brokers_watcher.gateways[0].connection_url); // 0 is dcmain, 1 is gyantal
        println!("Loop iteration. connUrl={}", conn_url);

        println!("On {}, new positions:", target_action_date);
        print!("New BUYS ({}):", new_buy_tickers.len());
        for stock in &new_buy_tickers {
            print!("  {} ({}), ", stock.attributes.name, stock.attributes.company_name);
        }
        
        print!("\nNew SELLS ({}):", new_sell_tickers.len());
        for stock in &new_sell_tickers {
            print!("  {} ({}), ", stock.attributes.name, stock.attributes.company_name);
        }
        println!(); // print newline for flushing the buffer. Otherwise the last line may not appear immediately.
        // io::stdout().flush().unwrap();  // Ensure immediate output, because it is annoying to wait for newline or buffer full
    }

    pub async fn start_fastrunning_loop(&mut self, brokers_watcher_guard: &Arc<Mutex<brokers_watcher::BrokersWatcher>>) {
        self.is_loop_active.store(true, Ordering::SeqCst);
        let is_loop_active_clone = self.is_loop_active.clone(); // Clone the Arc, not the AtomicBool
        println!("start_fastrunning_loop() started.");
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

                    tokio::time::sleep(tokio::time::Duration::from_millis(3750)).await;
                }
                println!("FastRunner thread stopping");
            });
        });
    }

    pub async fn stop_fastrunning_loop(&mut self) {
        self.is_loop_active.store(false, Ordering::SeqCst);
        println!("stop_fastrunning_loop() started.");
    }


}
