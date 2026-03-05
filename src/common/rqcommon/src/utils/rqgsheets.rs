use std::sync::OnceLock;

use csv::ReaderBuilder; // for reading the gsheets data
use google_sheets4::{api::ValueRange, hyper_rustls::HttpsConnectorBuilder, hyper_util::{client::legacy::Client, rt::TokioExecutor}, Sheets,}; // for write/modify the gsheet
use yup_oauth2::{ServiceAccountAuthenticator, ServiceAccountKey};

// Steps to create Google Sheets Service Account:
// 1. Go to https://console.cloud.google.com
// 2. Create new project (or use existing)
// 3. Enable Google Sheets API: → APIs & Services → Library → search "Google Sheets API" → Enable
// 4. Create Service Account: → APIs & Services → Credentials → Create Credentials → Service account
// 5. Give it any name → Create → skip optional steps → Done
// 6. Click the new service account → Keys tab → Add Key → Create new key → JSON → download the file
// 7. Share your Google Sheet with the service account email (you'll find the email in the JSON file → "client_email" field) → Open your sheet → Share → paste email → Editor access
pub struct RqGSheets {
    pub gsheet_client_email: String,
    pub gsheet_private_key: String,
}

// ---------- Global static variables ----------
pub static RQGSHEETS: OnceLock<RqGSheets> = OnceLock::new(); // Lock contains the global RqGSheets instance; OnceLock allows us to initialize it once at runtime

impl RqGSheets {
    pub fn init(client_email: &str, private_key: &str) {
        RQGSHEETS.set(RqGSheets {
            gsheet_client_email: client_email.to_string(),
            gsheet_private_key: private_key.to_string(),
        }).ok();
    }

    pub async fn set_single_cell(url: &str, col_num: u32, row_num: u32, value: &str,) -> Result<(), Box<dyn std::error::Error>> {
        // Convert Column Number to Letter
        let column_letter_ref = Self::column_number_to_letter(col_num); // e.g, col_num: 1 -> "A", 27 -> "AA"      
        let range = format!("Sheet1!{}{}", column_letter_ref, row_num); // Default sheet name = Sheet1
        // Prepare ValueRange for Update
        let update_value_range = ValueRange {
            major_dimension: Some("ROWS".to_string()),
            values: Some(vec![vec![value.into()]]),
            ..Default::default()
        };
        // Load Google Sheets Config
        let rqgsheet_config = match RQGSHEETS.get() {
            Some(cfg) => cfg,
            None => {
                log::error!("RqGsheets not initialized");
                return Err(std::io::Error::other("RqGsheets not initialized").into());
            }
        };
        // Build Service Account Key
        let service_account_key = ServiceAccountKey {
            client_email: rqgsheet_config.gsheet_client_email.clone(),
            private_key: rqgsheet_config.gsheet_private_key.clone(),
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
            key_type: None,
            project_id: None,
            private_key_id: None,
            client_id: None,
            auth_uri: None,
            auth_provider_x509_cert_url: None,
            client_x509_cert_url: None,
        };
        // Extract Spreadsheet ID
        let spreadsheet_id =  url
                .split("/d/")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .ok_or_else(|| {
                log::error!("Invalid Google Sheets URL");
                std::io::Error::other("Invalid Google Sheets URL")
            })?;
        // Build Authenticator
        let service_account_authenticator = ServiceAccountAuthenticator::builder(service_account_key)
            .build()
            .await?;
        // Build HTTP Transport
        let https_transport = HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();

        let http_client = Client::builder(TokioExecutor::new())
            .build(https_transport);
        // Create Google Sheets API Client
        let sheets_api_client = Sheets::new(http_client, service_account_authenticator);
        // Update Cell Value
        sheets_api_client.spreadsheets()
            .values_update(update_value_range, &spreadsheet_id, &range)
            .value_input_option("USER_ENTERED")
            .doit()
            .await?;
    
        Ok(())
    }
    
    pub async fn get_single_cell(url: &str, column_num: u32, row_num: u32) -> String { // column_num and row_num are expected to be 1-based indices, meaning (1,1) refers to the top-left cell of the sheet.
        if !url.contains("export?format=csv") {
            log::error!("Invalid URL. Please provide a Google Sheets CSV export URL (e.g:https://docs.google.com/spreadsheets/d/1wOY4OeoLbaYSfutiSc0elv26SVwLtBXqXnaNZ4YtggU/export?format=csv&gid=0).");
            return String::new();
        }

        let resp = match reqwest::get(url).await {
            Ok(resp) => resp,
            Err(err) => {
                log::error!("reqwest error: {}", err);
                return String::new();
            }
        };

        let csv_text = match resp.text().await {
            Ok(text) => text,
            Err(err) => {
                log::error!("csv_text error : {}", err);
                return String::new();
            }
        };

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(csv_text.as_bytes());
    
        //  Convert row_num and column_num to 0-based and get the cell value
        if let Some(Ok(record)) = reader.records().nth((row_num - 1) as usize) { // Convert to usize because iterator::nth() and record.get() require usize indices.
            return record.get((column_num - 1) as usize).unwrap_or("").to_string();
        }

        String::new()
    }

    pub async fn get_cell(url: &str, cell: &str) -> Result<String, Box<dyn std::error::Error>> {
        let csv_export_url = Self::convert_to_csv_export_url(url);
        let gsheet_data = Self::download_google_sheet(&csv_export_url).await?;
        let (row, col) = Self::parse_cell_id(cell);
        // Check if the requested row exists.
        let row_vec = match gsheet_data.get(row) {
            Some(r) => r,
            None => return Ok(String::new()),
        };

        // Check if the requested column exists in that row.
        let cell_value = match row_vec.get(col) {
            Some(v) => v.clone(),
            None => return Ok(String::new()),
        };

        Ok(cell_value)
    }

    fn convert_to_csv_export_url(url: &str) -> String { // url: "https://docs.google.com/spreadsheets/d/1NP8Tg08MqSoqd6wXSCus0rLXYG4TGPejzsGIP8r9YOk/edit?gid=0#gid=0";. e.g, sheet_id ="1NP8Tg08MqSoqd6wXSCus0rLXYG4TGPejzsGIP8r9YOk" and gid="0". The spreadsheet ID never changes regardless of whether the link contains /edit, /view, /preview, or /copy, so we extract the sheet_id and construct a CSV export URL from it. If gid is missing, default to 0.
        // Get spreadsheet id
        let sheet_id = url
            .split("/d/")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .unwrap_or("");

        // Get gid if present
        let gid = url
            .split("gid=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .unwrap_or("0");

        format!("https://docs.google.com/spreadsheets/d/{}/export?format=csv&gid={}", sheet_id, gid)
    }

    pub async fn download_google_sheet(csv_export_url: &str) -> Result<Vec<Vec<String>>, Box<dyn std::error::Error>> {
        let resp = reqwest::get(csv_export_url).await?;
        let csv_text = resp.text().await?;

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(csv_text.as_bytes());

        for record in reader.records() {
            let rec_str = record?;
            let mut row = Vec::with_capacity(rec_str.len());
            for cell in rec_str.iter() {
                row.push(cell.trim().to_string());
            }
            rows.push(row);
        }

        Ok(rows)
    }

    // Convert a cell like "B3" into numeric indexes. e.g: B3 -> row=2 col=1 (zero based)
    fn parse_cell_id(cell: &str) -> (usize, usize) {
        let mut col_label_str = String::new();
        let mut row_number_str = String::new();
        // Separate column letters and row numbers. e.g : "B3" -> col_label="B", row_numbers="3"
        for c in cell.chars() {
            if c.is_ascii_alphabetic() {
                col_label_str.push(c);
            } else if c.is_ascii_digit() {
                row_number_str.push(c);
            }
        }
        // Convert column label to a number using base-26 logic.
        let mut col_index: usize = 0;
        for c in col_label_str.to_ascii_uppercase().chars() {
            col_index = col_index * 26 + (c as usize - 'A' as usize + 1);
        }

        let row_index: usize = row_number_str.parse().unwrap_or(1); // Convert row string to number.
        (row_index - 1, col_index - 1) // Convert to zero-based indexing
    }

    fn column_number_to_letter(mut column_num: u32) -> String {
        let mut column_name = String::new();
        while column_num > 0 {
            let rem = ((column_num - 1) % 26) as u8;
            column_name.insert(0, (b'A' + rem) as char);
            column_num = (column_num - 1) / 26;
        }
        column_name
    }
}