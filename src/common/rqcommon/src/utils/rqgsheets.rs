use csv::ReaderBuilder;

pub struct RqGSheets;

impl RqGSheets {
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
}