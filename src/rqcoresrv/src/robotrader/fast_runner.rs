use chrono::{Datelike, Local, Utc};
use serde::Deserialize;
use std::{fmt::Write, collections::HashMap, fs, path::{Path, PathBuf}, time::{SystemTime}};
use rqcommon::{log_and_println, log_and_if_println, utils::time::{benchmark_elapsed_time_async}};

use broker_common::brokers_watcher::{RqOrder, RqOrderType};
use crate::robotrader::robo_trader::RoboTrader;

#[derive(Debug, Deserialize)]
pub struct PortfhistResponse {
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
    pub order_type: RqOrderType,
    pub action_date: String,
    pub ticker: String, // this is StockAttributes.name
    pub company_name: String,
    pub starting_weight: Option<Weight>,
    pub new_weight: Option<Weight>,
    pub price: Option<String>,
    pub pos_weight: f32, // calculated position weight in percentage (0.0 to 100.0)
    pub pos_market_value: f64, // calculated position market value in USD
}

    // ---------- Alpha Picks (AP) minimal model from JSON ----------
    #[derive(Debug, Deserialize)]
    pub struct AnalysisResponse { // works both in AP and PQP. They have the same JSON structure.
        pub data: Vec<Article>,
        pub included: Vec<IncludedItem>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    pub struct Article {
        pub id: String,
        #[serde(rename = "type")]
        pub type_: String,
        pub attributes: ArticleAttributes,
        pub relationships: ArticleRelationships,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    pub struct ArticleAttributes {
        #[serde(rename = "publishOn")]
        pub publish_on: String,
        pub title: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct ArticleRelationships {
        #[serde(rename = "primaryTickers")]
        pub primary_tickers: Option<RelationshipArray>,
    }

    #[derive(Debug, Deserialize)]
    pub struct RelationshipArray {
        pub data: Vec<RelationshipData>,
    }

    #[derive(Debug, Deserialize)]
    pub struct RelationshipData {
        pub id: String,
        #[serde(rename = "type")]
        pub type_: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct IncludedItem {
        pub id: String,
        #[serde(rename = "type")]
        pub type_: String,
        // We only care about tags; keep attributes generic to avoid many structs
        pub attributes: serde_json::Value,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    pub struct TagAttributes {
        pub slug: Option<String>,
        pub name: Option<String>,
        pub company: Option<String>,
    }


pub struct FastRunner {
    pub is_simulation: bool, // is true for simulation, false for real trading
    pub loop_sleep_ms_simulation: u32,
    pub loop_sleep_ms_realtrading: u32,
    pub has_trading_ever_started: bool,
    pub cookies: Option<String>,
    pub cookies_file_last_modtime: Option<SystemTime>,
    pub m_is_cookies_surely_working: bool,

    pub pqp_json_target_date_str: String,
    pub pqp_is_run_today: bool,

    pub ap_json_target_date_str:String,
    pub ap_is_run_today: bool,

    pub pqp_buy_pv: f64, // PQP PV for buys
    pub pqp_sell_pv: f64, // PQP PV for sells
    pub ap_buy_pv: f64, // AP PV for buys

    pub user_log: String, // accumulate summary of what we did in this FastRunner instance, and send it as email body in the end.
}

impl FastRunner {
    // When to schedule the service?
    // Until 2025-10-17: History page was updated at 9:30:00 AM ET on Mondays (after market open). Exactly at that second when market opens.
    // On 2025-10-27: Analysis page was updated 12:00:00 PM ET, but History page was only updated later, at 12:02:00 PM ET. So, run the loop from T-30sec to T+3min. Or implement reading Analysis page too.
    pub fn new() -> Self {
        Self {
            is_simulation: true, // at trading, change this to false. Also check if IbGateway is in ReadOnly mode.

            loop_sleep_ms_simulation: 3750, // usually 3750, that is 3.75s
            loop_sleep_ms_realtrading: 0, // usually 250ms (note that reqwest.client.get() is 500-700ms, so we don't have to sleep much here)
            has_trading_ever_started: false,
            // initialize cookies cache
            cookies: None,
            cookies_file_last_modtime: None,
            m_is_cookies_surely_working: false,

            pqp_json_target_date_str: String::new(),
            pqp_is_run_today: false,

            ap_json_target_date_str: String::new(),
            ap_is_run_today: false,

            pqp_buy_pv: 50000.0, // PQP PV for buys
            pqp_sell_pv: 30000.0, // PQP PV for sells
            ap_buy_pv: 50000.0, // AP PV for buys

            user_log: String::with_capacity(2048),  // Pre-allocate ~2KB for this StringBuilder.
        }
    }

    pub async fn init(&mut self) {
        self.pqp_ap_calculate_dates_and_pv();

        let dir = Path::new("../../../rqcore_data");
        tokio::fs::create_dir_all(dir).await.expect("create_dir_all() failed!"); // assure only once that the folder exists, so we don't have to do it in every loop iteration
    }

    pub fn pqp_ap_calculate_dates_and_pv(&mut self) {
        let now_utc = Utc::now().date_naive();

        let pqp_days_to_subtract = now_utc.weekday().days_since(chrono::Weekday::Mon) as i64; // equivalent to num_days_from_monday(). From Last Monday. If today is Monday, then it is 0.
        // let pqp_days_to_subtract = now_utc.weekday().days_since(chrono::Weekday::Tue) as i64; // If there is USA bank holiday on Monday, then use this
        let pqp_virtual_rebalance_date = now_utc - chrono::Duration::days(pqp_days_to_subtract); // always current or last Monday
        let pqp_real_rebalance_date = pqp_virtual_rebalance_date; // can be Tuesday; TODO: implement holiday checking.
        self.pqp_json_target_date_str = pqp_real_rebalance_date.format("%Y-%m-%d").to_string(); // Seek this in received JSON
        // let pqp_json_target_date_str = "2025-11-03".to_string(); // override for testing
        // Check if today is the real_rebalance_date
        self.pqp_is_run_today = now_utc == pqp_real_rebalance_date;
        // self.pqp_is_run_today = true; // override for testing
        let ap_virtual_rebalance_date = if now_utc.day() >= 15 { // virtual_rebalance_date as the 1st or 15th of month
            now_utc.with_day(15).unwrap()
        } else {
            now_utc.with_day(1).unwrap()
        };
        let ap_real_rebalance_date = match ap_virtual_rebalance_date.weekday() { // real_rebalance_date as virtual_rebalance_date or the first weekday after it if it falls on a weekend
            chrono::Weekday::Sat => ap_virtual_rebalance_date + chrono::Duration::days(2),
            chrono::Weekday::Sun => ap_virtual_rebalance_date + chrono::Duration::days(1),
            _ => ap_virtual_rebalance_date, // TODO: implement holiday checking.
        };
        // let ap_real_rebalance_date = ap_real_rebalance_date + chrono::Duration::days(1); // If there is USA bank holiday on Monday, then use this
        self.ap_json_target_date_str = ap_real_rebalance_date.format("%Y-%m-%d").to_string(); // Seek this in received JSON
        // Check if today is the real_rebalance_date
        self.ap_is_run_today = now_utc == ap_real_rebalance_date;
        // self.ap_is_run_today = true; // override for testing
        // Determine PV Portfolio Values to play. If both PQP and AP run today, then we can split the PV between them. If only one of them runs, then we can allocate all PV to that one.
        if self.pqp_is_run_today && self.ap_is_run_today { // future target: 70K+70K+60K short =200K.
            self.pqp_buy_pv = 70000.0;
            self.pqp_sell_pv = 60000.0;
            self.ap_buy_pv = 70000.0;
        } else if self.pqp_is_run_today {
            self.pqp_buy_pv = 140000.0;
            self.pqp_sell_pv = 60000.0;
            self.ap_buy_pv = 0.0;
        } else if self.ap_is_run_today {
            self.pqp_buy_pv = 0.0;
            self.pqp_sell_pv = 0.0;
            self.ap_buy_pv = 200000.0;
        } else {
            self.pqp_buy_pv = 0.0;
            self.pqp_sell_pv = 0.0;
            self.ap_buy_pv = 0.0;
        }

        // print everything for debugging. When it matures, then just log::info() it.
        log_and_println!("pqp_virtual_rebalance_date: {}, pqp_real_rebalance_date: {}, pqp_is_run_today: {}, ap_virtual_rebalance_date: {}, ap_real_rebalance_date: {}, ap_is_run_today: {}, pqp_buy_pv: {}, pqp_sell_pv: {}, ap_buy_pv: {}", 
            pqp_virtual_rebalance_date, pqp_real_rebalance_date, self.pqp_is_run_today, ap_virtual_rebalance_date, ap_real_rebalance_date, self.ap_is_run_today, self.pqp_buy_pv, self.pqp_sell_pv, self.ap_buy_pv);
        writeln!(self.user_log, "pqp_virtual_rebalance_date: {}, pqp_real_rebalance_date: {}, pqp_is_run_today: {}, ap_virtual_rebalance_date: {}, ap_real_rebalance_date: {}, ap_is_run_today: {}, pqp_buy_pv: {}, pqp_sell_pv: {}, ap_buy_pv: {}", 
            pqp_virtual_rebalance_date, pqp_real_rebalance_date, self.pqp_is_run_today, ap_virtual_rebalance_date, ap_real_rebalance_date, self.ap_is_run_today, self.pqp_buy_pv, self.pqp_sell_pv, self.ap_buy_pv).unwrap(); // write!() macro never panics for a String (infallible), so unwrap() is safe
    }

    // >2026-02-17: They updated the PQP.Analysis tabpage at 12:00 (but it only has tickerList for buy entries, not sell entries)
    // But they updated the PQP.Portfolio tab only at 12:15 (too late). If they do this always, we have to implement reading the Analysis tab.
    // But that will pose problems, as to avoid trading many times.
    // Also, the only way to get sell entries is to read the article and extract them, which is error-prone and an extra step.
    pub async fn get_new_transactions_pqp(&mut self) -> (String, Vec<TransactionEvent>) {
        // log_and_println!(">*{} get_new_transactions_pqp() started. target_date: {}", Utc::now().format("%H:%M:%S%.3f"), self.pqp_json_target_date_str);

        self.ensure_cookies_loaded(); // cookies are reloaded from file only if needed, if the file changed.
        self.m_is_cookies_surely_working = false;

        // tokio::spawn() puts the future onto Tokio’s runtime queue right away. And On a multi-thread runtime, it begins running on another worker thread almost immediately. On a current-thread runtime, it runs when the current task at an .await)
        let analysis_task = tokio::spawn(Self::get_new_transactions_from_analysis_pqp(
            self.cookies
                .as_deref()
                .expect("cookies not initialized")
                .to_string(),
            self.pqp_json_target_date_str.clone(),
        ));

        // This is not the Portfolio, but the Portfolio History tab, with the 1000 transactions.
        const URL_PQP_PORTFOLIO_HISTORY: &str = "https://seekingalpha.com/api/v3/quant_pro_portfolio/transactions?include=ticker.slug%2Cticker.name%2Cticker.companyName&page[size]=1000&page[number]=1";

        let mut body_text: String= String::new();
        benchmark_elapsed_time_async("reqwest.Client.get()", || async { // 1,800-3,600ms first, 500-700ms later with keep-alive
            // Build client with cookies
            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build().expect("Client::builder() failed!");

            let resp = client.get(URL_PQP_PORTFOLIO_HISTORY)
                .header("Cookie", self.cookies.as_deref().expect("cookies not initialized"))
                .send()
                .await
                .expect("reqwest client.get() failed!");

            // Get response as text first
            body_text = resp.text().await.expect("resp.text() failed!");
        }).await;
        
        // Save raw response
        let file_path = Self::save_response_file("fast_run_pqp_portfhist_src", &body_text).await;

        if body_text.len() < 1000 {
            if body_text.contains("Subscription is required") {
                log::error!("!Error. No permission, Update cookie file. See {}", file_path.display()); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
                writeln!(self.user_log, "!Error. No permission, Update cookie file. See {}", file_path.display()).unwrap(); // write!() macro never panics for a String (infallible), so unwrap() is safe
                return (self.pqp_json_target_date_str.clone(), Vec::new());
            } else if body_text.contains("captcha.js") {
                log::error!("!Error. Captcha required, Update cookie file AND handle Captcha in browser. See {}", file_path.display()); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
                writeln!(self.user_log, "!Error. Captcha required, Update cookie file AND handle Captcha in browser. See {}", file_path.display()).unwrap(); // write!() macro never panics for a String (infallible), so unwrap() is safe
                return (self.pqp_json_target_date_str.clone(), Vec::new());
            }
        }
        
        // Parse saved text as JSON
        let portfhist_response: PortfhistResponse = serde_json::from_str(&body_text).expect("serde_json::from_str() failed!");
        
        // Extract transactions list (Vec<Transaction>)
        let transactions = portfhist_response.data;
        if !transactions.is_empty() {  // if we have any transactions, cookies are surely working
            self.m_is_cookies_surely_working = true;
        }
        
        // Extract stocks dictionary (HashMap<String, Stock>)
        let mut stocks: HashMap<String, Stock> = HashMap::new();
        for stock in portfhist_response.included {
            stocks.insert(stock.id.clone(), stock);
        }

        log_and_if_println!(true, "Found {} transactions, {} stocks", transactions.len(), stocks.len());
        writeln!(self.user_log, "Found {} transactions, {} stocks", transactions.len(), stocks.len()).unwrap(); // write!() macro never panics for a String (infallible), so unwrap() is safe

        // Print all transactions
        // for transaction in &transactions {
        //     let ticker_id = &transaction.relationships.ticker.data.id;
        //     if let Some(stock) = stocks.get(ticker_id) {
        //         log_and_println!("Tr {}: {} {} {} weight of {} ({}) at ${}", transaction.attributes.id, transaction.attributes.actionDate, transaction.attributes.action, transaction.attributes.new_weight, stock.attributes.name, stock.attributes.companyName, transaction.attributes.price.as_deref().unwrap_or("N/A"));
        //     }
        // }

        // Collect transactions for the specific date
        let mut new_transaction_events: Vec<TransactionEvent> = Vec::new();
        for transaction in &transactions {
            // Skip if not our target date
            if transaction.attributes.action_date != self.pqp_json_target_date_str {
                continue;
            }
            // Skip rebalance transactions
            if transaction.attributes.rule.as_deref() == Some("rebalance") {
                continue;
            }
            let stock = stocks.get(&transaction.relationships.ticker.data.id);
            let order_type = match transaction.attributes.action.as_str() {
                "buy" => Some(RqOrderType::Buy),
                "sell" => Some(RqOrderType::Sell),
                _ => None,
            };

            if let (Some(stock), Some(order_type)) = (stock, order_type) {
                new_transaction_events.push(TransactionEvent {
                    transaction_id: transaction.id.clone(),
                    order_type,
                    action_date: transaction.attributes.action_date.clone(),
                    ticker: stock.attributes.name.clone(),
                    company_name: stock.attributes.company_name.clone(),
                    starting_weight: transaction.attributes.starting_weight.clone(),
                    new_weight: transaction.attributes.new_weight.clone(),
                    price: transaction.attributes.price.clone(),
                    pos_weight: 0.0,
                    pos_market_value: 0.0,
                });
            }
        }

        // if PortfHist doesn't give events (sometimes they update 15min later), then get the Buy entries from Analysis page (never delayed, but it only contains the Buys. Better than not trading due to 15min delay)
        // The only way to get the Sells is to read the article, and text NLP extract from it, which is error prone and another extra step, so ignore it for now.
        if new_transaction_events.is_empty() {
            match analysis_task.await {
                Ok(result) => return result,
                Err(err) => {
                    log::warn!("PQP analysis task failed: {}", err);
                }
            }
        }

        (self.pqp_json_target_date_str.clone(), new_transaction_events)
    }


    pub async fn test_http_download_pqp(&mut self) {
        let (target_action_date, new_transaction_events) = self.get_new_transactions_pqp().await;

        let (buy_count, sell_count) = Self::count_order_types(&new_transaction_events);

        // Print summary
        log_and_if_println!(true, "On {}, new transactions. Buys:{}, Sells:{}", target_action_date, buy_count, sell_count);
        for event in &new_transaction_events {
            log_and_if_println!(true, "  {} {} ({}, ${}) , ", event.order_type, event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));
        }
        // println!(); // print newline for flushing the buffer. Otherwise the last line may not appear immediately.
        // io::stdout().flush().unwrap();  // Ensure immediate output, because it is annoying to wait for newline or buffer full
    }

    fn determine_position_market_values_pqp_gyantal(&self, new_transaction_events: &mut Vec<TransactionEvent>) {
        let (buy_count, sell_count) = Self::count_order_types(new_transaction_events);

        let buy_pos_mkt_value = if buy_count > 0 {
            self.pqp_buy_pv / (buy_count as f64)
        } else {
            0.0
        };
        let sell_pos_mkt_value = if sell_count > 0 {
            self.pqp_sell_pv / (sell_count as f64)
        } else {
            0.0
        };

        for event in new_transaction_events.iter_mut() {
            event.pos_market_value = match event.order_type {
                RqOrderType::Buy => buy_pos_mkt_value,
                RqOrderType::Sell => sell_pos_mkt_value,
            };
        }
    }

    pub async fn fastrunning_loop_pqp_impl(&mut self) {

        let (target_action_date, mut new_transaction_events) = self.get_new_transactions_pqp().await;

        let num_new_events = new_transaction_events.len();
        if num_new_events == 0 {
            log_and_println!("No new transaction events on {}. Skipping trading.", target_action_date);
            return;
        }
        if num_new_events > 14 { // The most it was 7+7 = 14 trades in the past. And even if it is correct, if there are 8 buys and 8 sells, a lot of trading that I don't want. As in this spread out suggestion, the buying pressure is not that big.
            log::warn!("Something is wrong. Don't expect more than 14 events. num_new_events: {}. Skipping trading.", num_new_events);
            return;
        }

        self.determine_position_market_values_pqp_gyantal(&mut new_transaction_events); // replace it to blukucz if needed

        let rqorders = Self::build_rqorders(&new_transaction_events);

        // If we are here, there are events to trade. Assure that we trade only once.
        if self.has_trading_ever_started { // Assure that Trading only happens once per FastRunner instance. To avoid trading it many times.
            log::warn!("Trading already started. Skipping this iteration.");
            return;
        }
        self.has_trading_ever_started = true;

        RoboTrader::place_orders("SA_PQP", rqorders, self.is_simulation).await;
    }

    // >2026-02-17: They updated the AP.Analysis tabpage at 12:00, but there was no TickerList tag in it
    // The AP.Portfolio tab was updated only 15min later. (So, that is not a solution either)
    // One idea to implement: If we found an article that is exactly the right time. ("publishOn": "2026-02-17T12:01:21-05:00")
    // Ask Grok: "What is the ticker of the company mentioned in this summary:"
    pub async fn get_new_transactions_ap(&mut self) -> (String, Vec<TransactionEvent>) {
        // log_and_println!(">*{} get_new_transactions_ap() started. target_date: {}", Utc::now().format("%H:%M:%S%.3f"), self.ap_json_target_date_str);

        self.ensure_cookies_loaded(); // cookies are reloaded from file only if needed, if the file changed.
        self.m_is_cookies_surely_working = false;

        const URL_AP_ANALYSIS: &str = "https://seekingalpha.com/api/v3/service_plans/458/marketplace/articles?include=primaryTickers%2CsecondaryTickers%2CservicePlans%2CservicePlanArticles%2Cauthor%2CsecondaryAuthor";

        let mut body_text: String= String::new();
        benchmark_elapsed_time_async("reqwest.Client.get()", || async { // 1,800-3,600ms first, 500-700ms later with keep-alive
            // Build client with cookies
            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build().expect("Client::builder() failed!");

            let resp = client.get(URL_AP_ANALYSIS)
                .header("Cookie", self.cookies.as_deref().expect("cookies not initialized"))
                .send()
                .await
                .expect("reqwest client.get() failed!");

            // Get response as text first
            body_text = resp.text().await.expect("resp.text() failed!");
        }).await;
        
        // Save raw response
        let file_path = Self::save_response_file("fast_run_ap_src", &body_text).await;

        if !body_text.contains("\"isPaywalled\":false") { // Search ""isPaywalled":false". If it can be found, then it is good. Otherwise, we get the articles, but the primaryTickers will be empty.
            log::error!("!Error. No permission, Update cookie file. See {}. Sometimes it fixes itself in next query.", file_path.display()); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
            return (self.ap_json_target_date_str.clone(), Vec::new());
        } else if body_text.contains("captcha.js") {
            log::error!("!Error. Captcha required, Update cookie file AND handle Captcha in browser. See {}", file_path.display()); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
            return (self.ap_json_target_date_str.clone(), Vec::new());
        }

        // Parse saved text as JSON
        let analysis_response: AnalysisResponse = serde_json::from_str(&body_text).expect("serde_json::from_str() failed for AnalysisResponse!");

        // Build a lookup for included tag items: id -> (name, company)
        let mut tag_lookup: HashMap<String, (String, String)> = HashMap::new();
        for inc in &analysis_response.included {
            if inc.type_ == "tag" {
                if let Ok(tag_attr) = serde_json::from_value::<TagAttributes>(inc.attributes.clone()) {
                    let name = tag_attr.name.unwrap_or_default();
                    let company = tag_attr.company.unwrap_or_default();
                    tag_lookup.insert(inc.id.clone(), (name, company));
                }
            }
        }

        if !tag_lookup.is_empty() {  // if we have any "type": "tag" in the articles, cookies are surely working
            self.m_is_cookies_surely_working = true;
        }

        // Print each article with its primary ticker names and companies
        // for art in &ap_response.data {
        //     let publish_on = &art.attributes.publish_on;

        //     let mut tickers: Vec<String> = Vec::new();
        //     if let Some(rel) = &art.relationships.primary_tickers {
        //         for d in &rel.data {
        //             if d.type_ == "tag" {
        //                 if let Some((name, company)) = tag_lookup.get(&d.id) {
        //                     tickers.push(format!("{} ({})", name, company));
        //                 } else {
        //                     tickers.push(format!("{} (unknown)", d.id));
        //                 }
        //             }
        //         }
        //     }

        //     let tickers_str = if tickers.is_empty() { "-".to_string() } else { tickers.join(", ") };
        //     println!("AP Article: {} | {} | primary_tickers: {}", art.attributes.title, publish_on, tickers_str);
        // }

        // Collect transactions for the specific date. AP rebalance articles (1st/15th) typically contain buy entries.
        let mut new_transaction_events: Vec<TransactionEvent> = Vec::new();
        for article in &analysis_response.data {
            let publish_on = &article.attributes.publish_on; // 2025-10-15T12:00:23-04:00
            let publish_on_dateonly = &publish_on[0..10]; // extract "2025-10-15"
            if publish_on_dateonly != self.ap_json_target_date_str { // Skip if not our target date
                continue;
            }
            // For Analysis (Articles) page, both for AP or PQP, we consider all primary tickers as "buy" events
            if let Some(rel) = &article.relationships.primary_tickers {
                for d in &rel.data {
                    if d.type_ != "tag"
                        { continue; }
                    if let Some((name, company)) = tag_lookup.get(&d.id) {
                        // name can be a non USA (Canada) stock ticker, e.g. ""name": "CLS:CA". If name contains ':', we skip it
                        if name.contains(':')
                            { continue; }
                        new_transaction_events.push(TransactionEvent {
                            transaction_id: article.id.clone(),
                            order_type: RqOrderType::Buy,
                            action_date: publish_on_dateonly.to_string(),
                            ticker: name.clone(),
                            company_name: company.clone(),
                            starting_weight: None,
                            new_weight: None,
                            price: None,
                            pos_weight: 0.0,
                            pos_market_value: 0.0,
                        });
                    }
                }
            }
        }

        (self.ap_json_target_date_str.clone(), new_transaction_events)
    }

    pub async fn test_http_download_ap(&mut self) {
        let (target_action_date, new_transaction_events) = self.get_new_transactions_ap().await;

        // Print summary
        log_and_if_println!(true, "On {}, new transactions. Buys:{}, Sells:0", target_action_date, new_transaction_events.len());
        for event in &new_transaction_events {
            log_and_println!("  {} {} ({}, ${}) , ", event.order_type, event.ticker, event.company_name, event.price.as_deref().unwrap_or("N/A"));
        }
    }

    fn determine_position_market_values_ap_gyantal(&self, new_transaction_events: &mut Vec<TransactionEvent>) {
        let (buy_count, _) = Self::count_order_types(new_transaction_events);
        let buy_pos_mkt_value = if buy_count > 0 {
            self.ap_buy_pv / (buy_count as f64)
        } else {
            0.0
        };

        for event in new_transaction_events.iter_mut() {
            event.pos_market_value = match event.order_type {
                RqOrderType::Buy => buy_pos_mkt_value,
                RqOrderType::Sell => 0.0,
            };
        }
    }

    pub async fn fastrunning_loop_ap_impl(&mut self) {

        let (target_action_date, mut new_transaction_events) = self.get_new_transactions_ap().await;

        let num_new_events = new_transaction_events.len();
        if num_new_events == 0 {
            log_and_println!("No new transaction events on {}. Skipping trading.", target_action_date);
            return;
        }
        if num_new_events > 2 { // There should be 1 new buy per rebalance.
            log::warn!("Something is wrong. Don't expect more than 1-2 events. num_new_events: {}. Skipping trading.", num_new_events);
            return;
        }

        self.determine_position_market_values_ap_gyantal(&mut new_transaction_events); // replace it to blukucz if needed

        let rqorders = Self::build_rqorders(&new_transaction_events);

        // If we are here, there are events to trade. Assure that we trade only once.
        if self.has_trading_ever_started { // Assure that Trading only happens once per FastRunner instance. To avoid trading it many times.
            log::warn!("Trading already started. Skipping this iteration.");
            return;
        }
        self.has_trading_ever_started = true;

        RoboTrader::place_orders("SA_AP", rqorders, self.is_simulation).await;
    }

    // ---------- Helpers ----------

    const COOKIES_FILE_PATH: &'static str = "../../../rqcore_data/fast_run_1_headers.txt";
    // Elapsed Time of ensure_cookies_loaded(): 
    // first file read: 13,643us, 
    // full reread the same file: 700us,  
    // if checking only file_modified_time: 130us, 
    // if checking only m_is_cookies_surely_working and returning: 0.40us
    fn ensure_cookies_loaded(&mut self) {
        if self.m_is_cookies_surely_working { // skip 130us file operation, checking the file_modified_time if we are sure that cookies are working
            return;
        }

        let file_metadata = fs::metadata(Self::COOKIES_FILE_PATH).expect("metadata() failed for cookies file");
        let file_modified_time = file_metadata.modified().expect("modified() failed for cookies file");

        let need_reload = self.cookies.is_none()
            || self.cookies_file_last_modtime.map(|t| t != file_modified_time).unwrap_or(true);

        if need_reload {
            let content = fs::read_to_string(Self::COOKIES_FILE_PATH).expect("read_to_string() failed!");
            self.cookies = Some(content.trim().to_string());
            self.cookies_file_last_modtime = Some(file_modified_time);
            log::info!("Cookies loaded/refreshed from file.");
        }
    }

    fn count_order_types(events: &[TransactionEvent]) -> (usize, usize) {
        let buy_count = events
            .iter()
            .filter(|event| matches!(event.order_type, RqOrderType::Buy))
            .count();
        let sell_count = events
            .iter()
            .filter(|event| matches!(event.order_type, RqOrderType::Sell))
            .count();
        (buy_count, sell_count)
    }

    fn build_rqorders(events: &[TransactionEvent]) -> Vec<RqOrder> {
        events
            .iter()
            .map(|event| RqOrder {
                order_type: event.order_type,
                ticker: event.ticker.clone(),
                company_name: event.company_name.clone(),
                pos_market_value: event.pos_market_value,
                known_last_price: event.price.as_deref().and_then(|s| s.parse::<f64>().ok()),
            })
            .collect()
    }

    async fn save_response_file(file_prefix: &str, body_text: &str) -> PathBuf {
        let file_path = Path::new("../../../rqcore_data").join(format!("{}_{}.json", file_prefix, Local::now().format("%Y%m%dT%H%M%S")));
        tokio::fs::write(&file_path, body_text).await.expect("fs::write() failed!");
        file_path
    }

    // This Analysis finishes faster than the main Portfolio History download. PortfHistory: 400KB (first: 3800ms), Analysis: 85KB (first: 1200ms).
    async fn get_new_transactions_from_analysis_pqp(cookies: String, target_action_date: String) -> (String, Vec<TransactionEvent>) {
        const URL_PQP_ANALYSIS: &str = "https://seekingalpha.com/api/v3/quant_pro_portfolio/articles?include=primaryTickers%2CsecondaryTickers%2Cauthor%2CsecondaryAuthor&lang=en";

        let mut body_text: String = String::new();
        benchmark_elapsed_time_async("reqwest.Client.get() - PQP.Analysis", || async {
            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build().expect("Client::builder() failed!");

            let resp = client.get(URL_PQP_ANALYSIS)
                .header("Cookie", &cookies)
                .send()
                .await
                .expect("reqwest client.get() failed!");

            body_text = resp.text().await.expect("resp.text() failed!");
        }).await;

        let file_path = Self::save_response_file("fast_run_pqp_analysis_src", &body_text).await;

        if !body_text.contains("\"isPaywalled\":false") { // Search ""isPaywalled":false". If it can be found, then it is good. Otherwise, we get the articles, but the primaryTickers will be empty.
            log::error!("!Error. No permission, Update cookie file. See {}. Sometimes it fixes itself in next query.", file_path.display()); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
            return (target_action_date, Vec::new());
        } else if body_text.contains("captcha.js") {
            log::error!("!Error. Captcha required, Update cookie file AND handle Captcha in browser. See {}", file_path.display()); // we don't have to terminate the infinite Loop. The admin can update the cookie file and the next iteration will notice it.
            return (target_action_date, Vec::new());
        }

        // Parse saved text as JSON
        let analysis_response: AnalysisResponse = serde_json::from_str(&body_text).expect("serde_json::from_str() failed for AnalysisResponse!");

        // Build a lookup for included tag items: id -> (name, company)
        let mut tag_lookup: HashMap<String, (String, String)> = HashMap::new();
        for inc in &analysis_response.included {
            if inc.type_ == "tag" {
                if let Ok(tag_attr) = serde_json::from_value::<TagAttributes>(inc.attributes.clone()) {
                    let name = tag_attr.name.unwrap_or_default();
                    let company = tag_attr.company.unwrap_or_default();
                    tag_lookup.insert(inc.id.clone(), (name, company));
                }
            }
        }

        // Collect transactions for the specific date. AP rebalance articles (1st/15th) typically contain buy entries.
        let mut new_transaction_events: Vec<TransactionEvent> = Vec::new();
        for article in &analysis_response.data {
            let publish_on = &article.attributes.publish_on; // 2025-10-15T12:00:23-04:00
            let publish_on_dateonly = &publish_on[0..10]; // extract "2025-10-15"
            if publish_on_dateonly != target_action_date { // Skip if not our target date
                continue;
            }
            // For Analysis (Articles) page, both for AP or PQP, we consider all primary tickers as "buy" events
            if let Some(rel) = &article.relationships.primary_tickers {
                for d in &rel.data {
                    if d.type_ != "tag"
                        { continue; }
                    if let Some((name, company)) = tag_lookup.get(&d.id) {
                        // name can be a non USA (Canada) stock ticker, e.g. ""name": "CLS:CA". If name contains ':', we skip it
                        if name.contains(':')
                            { continue; }
                        new_transaction_events.push(TransactionEvent {
                            transaction_id: article.id.clone(),
                            order_type: RqOrderType::Buy,
                            action_date: publish_on_dateonly.to_string(),
                            ticker: name.clone(),
                            company_name: company.clone(),
                            starting_weight: None,
                            new_weight: None,
                            price: None,
                            pos_weight: 0.0,
                            pos_market_value: 0.0,
                        });
                    }
                }
            }
        }

        (target_action_date, new_transaction_events)
    }
}
