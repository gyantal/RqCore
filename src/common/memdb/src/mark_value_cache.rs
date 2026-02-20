// MarkValue terminology explanation.
// IB uses Mark Price for Estimated Mark-to-Market (EMM) calculations. Mark is a better term than LastPrice, LastTradePrice, RealtimePrice, EstPrice.
// We use Value instead of Price, because VIX and other indices or exchange rates are not prices, but values.
// Don't name it LastPrice. Because it might refer to LastTrade Price.
// MarkPrice (EstPrice) can be calculated from Ask/Bid, even if there is no Last Trade price (as Options may not trade even 1 contracts for days, so there is no Last Trade, but we estimate the price from RT Ask/Bid)
// MarkValue is a similar concept to IB's MarkPrice. An estimated price (Mark-to-Mark) that is used for margin calculations. It has a discretionary calculation, and can based on Ask/Bid/LastTrade (if happened recently)

use std::{collections::HashMap, sync::{LazyLock, Mutex}};
use chrono::{DateTime, Utc};

pub type MarkValue = f64;
pub type MarkTime = DateTime<Utc>;

pub static RQ_MARK_VALUE_CACHE: LazyLock<Mutex<MarkValueCache>> = LazyLock::new(|| Mutex::new(MarkValueCache::new()));

pub struct MarkValueCache {
    pub ticker_universe_csv: String,
    pub mark_timevalues: HashMap<String, (MarkValue, MarkTime)>,
}

impl MarkValueCache {
    pub fn new() -> Self {
        Self {
            ticker_universe_csv: "TAYD,CGAU,DTEGY,ALIZY".to_string(),
            mark_timevalues: HashMap::new(),
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
            self.mark_timevalues
                .insert(ticker.to_string(), (f64::INFINITY, DateTime::<Utc>::UNIX_EPOCH));
        }
    }

    pub fn update(&mut self) {
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