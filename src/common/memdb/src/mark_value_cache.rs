// MarkValue terminology explanation.
// IB uses Mark Price for Estimated Mark-to-Market (EMM) calculations. Mark is a better term than LastPrice, LastTradePrice, RealtimePrice, EstPrice.
// We use Value instead of Price, because VIX and other indices or exchange rates are not prices, but values.
// Don't name it LastPrice. Because it might refer to LastTrade Price.
// MarkPrice (EstPrice) can be calculated from Ask/Bid, even if there is no Last Trade price (as Options may not trade even 1 contracts for days, so there is no Last Trade, but we estimate the price from RT Ask/Bid)
// MarkValue is a similar concept to IB's MarkPrice. An estimated price (Mark-to-Mark) that is used for margin calculations. It has a discretionary calculation, and can based on Ask/Bid/LastTrade (if happened recently)

// yfinance-rs's StreamBuilder Websocket implementation.
// YF websocket only gives data if there was a real volume traded (non-liquid stocks might not get data, no matter how long we wait. E.g. PRE has about 1 trade per 10 minutes)
// For 95% of the stocks, 5sec warmup is enough. However, consider 20 sec warmup to have some chance to get data for non-liquid stocks.
// TODO: consider using yfinance-rs's YF REST API to get the initial MarkValue as a snapshot, and then use Websocket stream to update it in real-time during market hours.
// This way we can have MarkValue for non-liquid stocks as well, even if there is hardly any trade during the day.
use std::{collections::HashMap, sync::{LazyLock, Mutex}, time::Duration};
use tokio::task::JoinHandle;
use chrono::{DateTime, Utc};
use yfinance_rs::{StreamBuilder, StreamMethod, YfClient};

use rqcommon::log_and_println;

pub type MarkValue = f64;
pub type MarkTime = DateTime<Utc>;

pub static RQ_MARK_VALUE_CACHE: LazyLock<Mutex<MarkValueCache>> = LazyLock::new(|| Mutex::new(MarkValueCache::new()));

pub struct MarkValueCache {
    pub ticker_universe_csv: String,
    pub mark_timevalues: HashMap<String, (MarkValue, MarkTime)>,
    quote_stream_task: Option<JoinHandle<()>>, // parrallel stream task to update mark values in real-time during market hours.
    quote_stream_users: u16,
}

impl MarkValueCache {
    pub fn new() -> Self {
        Self {
            // collect tickers. Potential Sells from SA Portfolio page. Potential Buys from Balazs.
            ticker_universe_csv: "ARREF,NESR,ORLA,IRS,CSTM,ALGT,SUHJY,NOAH,LASR,FXC,NLCP,FRO,STTK,EGO,RY,GM,FN,AIR,CRDO,PRE,CIEN,GRAL,PARR".to_string(),
            mark_timevalues: HashMap::new(),
            quote_stream_task: None,
            quote_stream_users: 0,
        }
    }

    pub fn init(&mut self) {
        self.mark_timevalues.clear();

        for ticker in self
            .ticker_universe_csv
            .split(',')
            .map(|ticker| ticker.trim())
            .filter(|ticker| !ticker.is_empty())
        {
            self.mark_timevalues.insert(ticker.to_string(), (f64::NAN, DateTime::<Utc>::UNIX_EPOCH));
        }
    }

    pub fn start_quote_stream(&mut self) {
        log_and_println!("MarkValueCache.start_quote_stream(): ! Only receives prices during Market Hours...");
        self.quote_stream_users = self.quote_stream_users.saturating_add(1);

        if let Some(task) = self.quote_stream_task.as_ref() {
            if !task.is_finished() {
                log::info!("MarkValueCache.start_quote_stream(): stream already running (users: {}).", self.quote_stream_users);
                return;
            }
        }

        self.quote_stream_task = None;

        let symbols: Vec<String> = self.ticker_universe_csv.split(',').map(|ticker| ticker.trim())
            .filter(|ticker| !ticker.is_empty()).map(|ticker| ticker.to_string()).collect();

        if symbols.is_empty() {
            log::warn!("MarkValueCache.start_quote_stream(): ticker_universe_csv is empty. Not starting background task.");
            self.quote_stream_users = self.quote_stream_users.saturating_sub(1);
            return;
        }

        let client = YfClient::default();

        // YF Websocket stream is about 1.5 seconds late only. Don't compare it to Windows time (4 seconds early), but IB TWS time.
        let (handle, mut receiver) = match StreamBuilder::new(&client)
            .symbols(symbols.clone())
            .method(StreamMethod::WebsocketWithFallback)
            .interval(Duration::from_secs(1))
            .diff_only(true)
            .start() {
                Ok((handle, receiver)) => (handle, receiver),
                Err(err) => {
                    log::error!("MarkValueCache.start_quote_stream(): failed to start stream: {err}");
                    self.quote_stream_users = self.quote_stream_users.saturating_sub(1);
                    return;
                }
            };

        let stream_task = tokio::spawn(async move {
            let _stream_handle = handle;

            while let Some(update) = receiver.recv().await {
                let symbol = update.symbol.to_string();
                let mark_value = update.price.as_ref().map(yfinance_rs::core::conversions::money_to_f64).unwrap_or(f64::NAN);
                if mark_value.is_nan() { // if NaN, there is no point storing it.
                    continue;
                }
                let mark_time = update.ts;

                if let Ok(mut mark_value_cache) = RQ_MARK_VALUE_CACHE.lock() {
                    mark_value_cache.mark_timevalues.insert(symbol.clone(), (mark_value, mark_time));
                }

                let vol = update.volume.map(|v| format!(" (vol Δ: {v})")).unwrap_or_default();
                println!("{}: ${:.2}{} (timestamp: {})", symbol, mark_value, vol, mark_time.format("%Y-%m-%d %H:%M:%S"));
            }

            log::warn!("MarkValueCache.start_quote_stream(): stream receiver ended.");
        });

        self.quote_stream_task = Some(stream_task);
        log::info!("MarkValueCache.start_quote_stream(): started for {} ticker(s) (users: {}).", symbols.len(), self.quote_stream_users);
    }

    pub fn stop_quote_stream(&mut self) {
        if self.quote_stream_users == 0 {
            log::warn!("MarkValueCache.stop_quote_stream(): called with no active users.");
            return;
        }

        self.quote_stream_users -= 1;

        if self.quote_stream_users > 0 {
            log::info!("MarkValueCache.stop_quote_stream(): user released stream (users left: {}).", self.quote_stream_users);
            return;
        }

        let Some(task) = self.quote_stream_task.take() else {
            log::info!("MarkValueCache.stop_quote_stream(): no stream task to stop.");
            return;
        };

        task.abort();
        log::info!("MarkValueCache.stop_quote_stream(): stop requested.");
    }

    pub fn is_quote_stream_running(&self) -> bool {
        self.quote_stream_task.as_ref().map(|task| !task.is_finished()).unwrap_or(false)
    }

    pub fn get_mark_value(&self, ticker: &str) -> MarkValue {
        self.mark_timevalues
            .get(ticker)
            .map(|(value, _time)| *value)
            .unwrap_or(f64::NAN)
    }

    pub fn get_mark_timevalue(&self, ticker: &str) -> (MarkValue, MarkTime) {
        self.mark_timevalues
            .get(ticker)
            .map(|(value, time)| (*value, time.clone()))
            .unwrap_or((f64::NAN, DateTime::<Utc>::UNIX_EPOCH))
    }

    pub fn get_mark_values<'a, I>(&'a self, tickers: I) -> impl Iterator<Item = (&'a str, MarkValue)> + 'a
    where
        I: IntoIterator<Item = &'a str> + 'a,
    {
        tickers
            .into_iter()
            .map(move |ticker| (ticker, self.get_mark_value(ticker)))
    }

    pub fn get_mark_timevalues<'a, I>(&'a self, tickers: I) -> impl Iterator<Item = (&'a str, MarkValue, MarkTime)> + 'a
    where
        I: IntoIterator<Item = &'a str> + 'a,
    {
        tickers
            .into_iter()
            .map(move |ticker| {
                let (value, time) = self.get_mark_timevalue(ticker);
                (ticker, value, time)
            })
    }
}