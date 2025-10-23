use chrono::Local;
use std::path::PathBuf;
use std::path::Path;
use serde::Deserialize;
use std::collections::HashMap;

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



#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Stock {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub attributes: StockAttributes,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct StockAttributes {
    pub name: String,
    #[serde(rename = "companyName")]
    pub company_name: String,
}

pub async fn test_http_download() -> Result<(), Box<dyn std::error::Error>> {
    println!("test_http_download() started.");
    
    // Load cookies from file
    let cookies = std::fs::read_to_string("../../../rqcore_data/fast_run_1_headers.txt")?;
    
    // Target URL
    const URL: &str = "https://seekingalpha.com/api/v3/quant_pro_portfolio/transactions?include=ticker.slug%2Cticker.name%2Cticker.companyName&page[size]=1000&page[number]=1";

    let ts = Local::now().format("%Y%m%dT%H%M%S").to_string();
    let dir = Path::new("../../../rqcore_data");
    let path: PathBuf = dir.join(format!("fast_run_1_src_{}.json", ts));

    tokio::fs::create_dir_all(dir).await?;

    // Build client with cookies
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let resp = client.get(URL)
        .header("Cookie", cookies.trim())
        .send()
        .await?;

    // Get response as text first
    let body_text = resp.text().await?;
    
    // Save raw response
    tokio::fs::write(&path, &body_text).await?;
    println!("Saved raw JSON response to {}", path.display());
    
    // Parse saved text as JSON
    let api_response: ApiResponse = serde_json::from_str(&body_text)?;
    
    // Extract transactions list (Vec<Transaction>)
    let transactions = api_response.data;
    
    // Extract stocks dictionary (HashMap<String, Stock>)
    let mut stocks: HashMap<String, Stock> = HashMap::new();
    for stock in api_response.included {
        stocks.insert(stock.id.clone(), stock);
    }

    println!("Found {} transactions, {} stocks", transactions.len(), stocks.len());
    // Print first transaction example
    if let Some(first_tx) = transactions.first() {
        println!("First transaction: id={}, action={}, price={:?}, tickerId={}", 
            first_tx.attributes.id, 
            first_tx.attributes.action, 
            first_tx.attributes.price,
            first_tx.relationships.ticker.data.id
        );
    }

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
    let target_action_date: &str = "2025-10-17"; // Friday date
    let mut new_buy_tickers: Vec<&Stock> = Vec::new();
    let mut new_sell_tickers: Vec<&Stock> = Vec::new();

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
                new_buy_tickers.push(stock);
            },
            "sell" => if let Some(stock) = stock {
                new_sell_tickers.push(stock);
            },
            _ => {}
        }
    }

    // Print summary
    println!("\nOn {}, new positions:", target_action_date);
    println!("New BUYS ({}):", new_buy_tickers.len());
    for stock in &new_buy_tickers {
        println!("  {} ({})", stock.attributes.name, stock.attributes.company_name);
    }
    
    println!("New SELLS ({}):", new_sell_tickers.len());
    for stock in &new_sell_tickers {
        println!("  {} ({})", stock.attributes.name, stock.attributes.company_name);
    }

    Ok(())
}
