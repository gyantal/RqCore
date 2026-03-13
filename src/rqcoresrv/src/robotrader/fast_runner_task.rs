use {
    std::{fmt::Write, future::Future, pin::Pin, sync::{Mutex}},
    chrono::{DateTime, NaiveTime, TimeZone, Utc},
    chrono_tz::US::Eastern,
};

use rqcommon::{log_and_println, rqhelper::MutexExt, utils::{rqemail::RqEmail, time::localtimeonly2future_datetime_tz}};
use memdb::mark_value_cache::RQ_MARK_VALUE_CACHE;

use crate::{get_rqcore_config, robotrader::fast_runner::FastRunner, services::rqtask_scheduler::RqTask};

// TODO: There is a lot of code duplication for FastRunnerPqpTask FastRunnerApTask. Unify them to FastRunnerPqpApTask or FastRunnerSaTask

// ---------- FastRunner PQP (daily 11:59 ET) ----------
pub struct FastRunnerPqpTask {
    name: String,
    next_time: Mutex<DateTime<Utc>>,
    pub is_manual_user_forcerun: bool,
}

impl FastRunnerPqpTask {
    pub fn new() -> Self {
        let next_utc = Self::get_next_trigger_time_impl();
        FastRunnerPqpTask {
            name: "FastRunnerPqpTask".to_string(),
            next_time: Mutex::new(next_utc),
            is_manual_user_forcerun: false,
        }
    }

    fn get_next_trigger_time_impl() -> DateTime<Utc> { // we run 3 times daily: 2x Simulation, 1x RealTrading at 11:01 ET, 11:30 ET, 11:59 ET
        let tz = Eastern;
        let targets_tz = [
            // NaiveTime::from_hms_opt(15, 26, 00).unwrap(), // for manual test
            NaiveTime::from_hms_opt(9, 45, 10).unwrap(), // USA market opens at 9:30 ET, so around 9:45 ET is the earliest.
            NaiveTime::from_hms_opt(11, 1, 10).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 10).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 10).unwrap(), // consider mark_value_cache warm up time, so trigger earlier
        ];

        targets_tz
            .into_iter()
            .map(|time| localtimeonly2future_datetime_tz(tz, time).to_utc())
            .min() // pick the earliest UTC time
            .unwrap()
    }
}

impl RqTask for FastRunnerPqpTask {
    fn name(&self) -> &str { &self.name }

    fn get_next_trigger_time(&self) -> DateTime<Utc> {
        *self.next_time.lock().unwrap()
    }

    fn update_next_trigger_time(&self) {
        let next_utc = Self::get_next_trigger_time_impl();
        let mut next = self.next_time.lock().unwrap();
        *next = next_utc;
    }

    fn run(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        // let this = self;
        Box::pin(async move {
             log_and_println!("{} FastRunnerPqpTask run() started", Utc::now().format("%H:%M:%S%.3f"));

            let mut fast_runner = FastRunner::new();
            fast_runner.init().await;

            let tz_et = Eastern;
            let now_et = Utc::now().with_timezone(&tz_et);
            let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
            let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
            let num_seconds_to_noon = (now_et - target_et).num_seconds().abs();
            let mut is_first_scheduled_today = num_seconds_to_noon > 2 * 60 * 60; // If current time from 12:00 ET is more than 2 hours. (at 9:45 ET)
            let is_last_scheduled_today = num_seconds_to_noon < 90; // If current time from 12:00 ET is less than 90 seconds, then set it to true.

            fast_runner.is_simulation = !is_last_scheduled_today; // live trading is only if it is the last scheduled time (11:59 ET)

            if self.is_manual_user_forcerun {
                fast_runner.pqp_is_run_today = true;
                fast_runner.is_simulation = true; // whatever is the calculation, force simulation in this mode.
                is_first_scheduled_today = true;
            }

            if !fast_runner.pqp_is_run_today {
                log_and_println!("Today is not the scheduled day for FastRunnerPqpTask");
                return;
            }

            writeln!(fast_runner.user_log, "{}: FastRunnerPqpTask run() loop started. Json target date: {}, is_simulation: {}", Utc::now().format("%H:%M:%S"), fast_runner.pqp_json_target_date_str, fast_runner.is_simulation).unwrap(); // write!() macro never panics for a String (infallible), so unwrap() is safe

            if is_first_scheduled_today {
                if let Some(email_to_address) = get_rqcore_config().get("email_gyant") {
                    let pqp_screener_tickers = FastRunner::get_sa_screener_result_tickers(r#"{"filter":{"quant_rating":{"in":["strong_buy"]},"quant_rating_days":{"in":[{"gte":25}]}},"page":1,"per_page":200,"sort":null,"total_count":true,"type":"stock"}"#).await;
                    println!("SA PQP Screener Tickers (#{:?}): {:?}", pqp_screener_tickers.len(), pqp_screener_tickers);

                    let pqp_position_tickers = FastRunner::get_pqp_positions_tickers().await;
                    println!("SA PQP Position Tickers (#{:?}): {:?}", pqp_position_tickers.len(), pqp_position_tickers);

                    let candidate_tickers = FastRunner::get_sa_candidate_tickers().await;
                    println!("SA PQP CandidateTickers (#{:?}, #{:?}): {:?}", candidate_tickers.0.len(), candidate_tickers.1.len(), candidate_tickers);

                    let candidate_tickers_email_body = format!("SA PQP Screener Tickers (#{:?}): {:?}\n\nSA PQP Position Tickers (#{:?}): {:?}\n\nSA PQP CandidateTickers (#{:?}, #{:?}): {:?}",
                        pqp_screener_tickers.len(), pqp_screener_tickers, pqp_position_tickers.len(), pqp_position_tickers, candidate_tickers.0.len(), candidate_tickers.1.len(), candidate_tickers);

                    if let Err(err) = RqEmail::send_text(email_to_address, "RqCore: SA PQP/AP Candidate Tickers", candidate_tickers_email_body.as_str()).await {
                        log_and_println!("RqEmail::send_text() failed: {}", err);
                    }
                }
            }

            { // keep the mutex lock scope as small as possible
                let mut mark_value_cache = RQ_MARK_VALUE_CACHE.lock_ignore_poison();
                mark_value_cache.start_quote_stream();
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(20000)).await; // let mark_value_cache warm up for 20sec to get some rt-prices

            let loop_endtime = tokio::time::Instant::now()
                + if fast_runner.is_simulation { tokio::time::Duration::from_secs(30)}
                    else { tokio::time::Duration::from_secs(4 * 60 + 30) }; // 2025-12-01: AP/Analysis tab published at 12:01:15 ET (late), the AP history was published 30 seconds earlier

            // >Example running time at trading:
            // 17:00:02.952 FastRunnerPqpTask run(): Loop iteration (IsSimu:false)
            // Elapsed Time of reqwest.Client.get(): 2,054ms. // SA refreshed the page (high demand), or it couldn't come from RAM cache, so 600ms => 2000ms.
            // 17:00:06.433 FastRunnerPqpTask run() ended
            // it was 2 trades sent. It took 3.5 seconds (including downloading the page (2sec), getting the 2 prices, sending the order)
            // as 2sec was the download URL time, RqCore handles it in 1.5sec with 2 price query and 2 order. So, about 500ms per stock.
            while tokio::time::Instant::now() < loop_endtime { // if the loop runs more than 4 minutes 30 seconds, then finish the loop
                log_and_println!(">*{}: FastRunnerPqpTask run() loop iteration started. Json target date: {}, is_simulation: {}", Utc::now().format("%H:%M:%S%.3f"), fast_runner.pqp_json_target_date_str, fast_runner.is_simulation);

                fast_runner.fastrunning_loop_pqp_impl().await;
                if self.is_manual_user_forcerun { // User forcerun only wants to test 1 loop. And if "No new buy/sell events on {}. Skipping trading." happens, then has_trading_ever_started cannot be used to exits after 1 loop, because it will never be true.
                    break;
                }

                if fast_runner.has_trading_ever_started {
                    log_and_println!("FastRunnerPqpTask: Trading has started, exiting the loop.");
                    break;
                }

                let sleep_ms = if fast_runner.is_simulation { fast_runner.loop_sleep_ms_simulation } else { fast_runner.loop_sleep_ms_realtrading };
                if sleep_ms > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(sleep_ms.into())).await;
                }
            }
            log_and_println!("{} FastRunnerPqpTask run() ended", Utc::now().format("%H:%M:%S%.3f"));

            { // keep the mutex lock scope as small as possible
                let mut mark_value_cache = RQ_MARK_VALUE_CACHE.lock_ignore_poison();
                mark_value_cache.stop_quote_stream();
            }

            if let Some(email_to_address) = get_rqcore_config().get("email_gyant") {
                log_and_println!("Sending email. Might take 18 sec 'sometimes' (normally: 1-2.5sec)...(In single-threaded Tokio, Console or any messages are not handled. Investigate later: 1. We need an async RqEmail anyway (even if it is only 2 sec). 2. Why does it take 18sec)");
                // In the final stage: just send email about live trades run(), but not the previous 3x simulations (except if there was an error in simulation).

                let start = tokio::time::Instant::now();
                if let Err(err) = RqEmail::send_text(email_to_address, "RqCore: FastRunnerPqpTask run() ended", fast_runner.user_log.as_str()).await {
                    log_and_println!("RqEmail::send_text() failed: {}", err);
                } else {
                    log_and_println!("Elapsed Time of RqEmail::send_text(): {:.2}us", start.elapsed().as_secs_f64() * 1_000_000.0);
                }
            }
        })
    }
}


// ---------- FastRunner AP (daily 11:59 ET) ----------
pub struct FastRunnerApTask {
    name: String,
    next_time: Mutex<DateTime<Utc>>,
    pub is_manual_user_forcerun: bool,
}

impl FastRunnerApTask {
    pub fn new() -> Self {
        let next_utc = Self::get_next_trigger_time_impl();
        FastRunnerApTask {
            name: "FastRunnerApTask".to_string(),
            next_time: Mutex::new(next_utc),
            is_manual_user_forcerun: false,
        }
    }

    fn get_next_trigger_time_impl() -> DateTime<Utc> { // we run 3 times daily: 2x Simulation, 1x RealTrading at 11:01 ET, 11:30 ET, 11:59 ET
        let tz = Eastern;
        let targets_tz = [
            // NaiveTime::from_hms_opt(15, 26, 10).unwrap(), // for manual test
            NaiveTime::from_hms_opt(9, 50, 10).unwrap(), // USA market opens at 9:30 ET, so around 9:45 ET is the earliest.
            NaiveTime::from_hms_opt(11, 5, 20).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 20).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 20).unwrap(), // consider mark_value_cache warm up time, so trigger earlier
        ];

        targets_tz
            .into_iter()
            .map(|t| localtimeonly2future_datetime_tz(tz, t).to_utc())
            .min() // pick the earliest UTC time
            .unwrap()
    }
}

impl RqTask for FastRunnerApTask {
    fn name(&self) -> &str { &self.name }

    fn get_next_trigger_time(&self) -> DateTime<Utc> {
        *self.next_time.lock().unwrap()
    }

    fn update_next_trigger_time(&self) {
        let next_utc = Self::get_next_trigger_time_impl();
        let mut next = self.next_time.lock().unwrap();
        *next = next_utc;
    }

    fn run(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        // let this = self;
        Box::pin(async move {
            log_and_println!("{} FastRunnerApTask run() started", Utc::now().format("%H:%M:%S%.3f"));

            let mut fast_runner = FastRunner::new();
            fast_runner.init().await;

            let tz_et = Eastern;
            let now_et = Utc::now().with_timezone(&tz_et);
            let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
            let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
            let is_live_trading_based_on_closeto_noon = (now_et - target_et).num_seconds().abs() < 90; // If current time from 12:00 ET is less than 90 seconds, then set it to true.

            fast_runner.is_simulation = !is_live_trading_based_on_closeto_noon;

            if self.is_manual_user_forcerun {
                fast_runner.ap_is_run_today = true;
                fast_runner.is_simulation = true; // whatever is the calculation, force simulation in this mode.
            }

            if !fast_runner.ap_is_run_today {
                log_and_println!("Today is not the scheduled day for FastRunnerApTask");
                return;
            }

            writeln!(fast_runner.user_log, "{}: FastRunnerApTask run() loop started. Json target date: {}, is_simulation: {}", Utc::now().format("%H:%M:%S"), fast_runner.ap_json_target_date_str, fast_runner.is_simulation).unwrap(); // write!() macro never panics for a String (infallible), so unwrap() is safe

            { // keep the mutex lock scope as small as possible
                let mut mark_value_cache = RQ_MARK_VALUE_CACHE.lock_ignore_poison();
                mark_value_cache.start_quote_stream();
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(20000)).await; // let mark_value_cache warm up for 20sec to get some rt-prices

            let loop_endtime = tokio::time::Instant::now()
                + if fast_runner.is_simulation { tokio::time::Duration::from_secs(30)}
                    else { tokio::time::Duration::from_secs(4 * 60 + 30) }; // 2025-12-01: AP/Analysis tab published at 12:01:15 ET (late), the AP history was published 30 seconds earlier

            while tokio::time::Instant::now() < loop_endtime { // if the loop runs more than 4 minutes 30 seconds, then finish the loop
                log_and_println!(">*{}: FastRunnerApTask run() loop iteration started. Json target date: {}, is_simulation: {}", Utc::now().format("%H:%M:%S%.3f"), fast_runner.ap_json_target_date_str, fast_runner.is_simulation);

                fast_runner.fastrunning_loop_ap_impl().await;
                if self.is_manual_user_forcerun { // User forcerun only wants to test 1 loop. And if "No new buy/sell events on {}. Skipping trading." happens, then has_trading_ever_started cannot be used to exits after 1 loop, because it will never be true.
                    break;
                }

                if fast_runner.has_trading_ever_started {
                    log_and_println!("FastRunnerApTask: Trading has started, exiting the loop.");
                    break;
                }

                let sleep_ms = if fast_runner.is_simulation { fast_runner.loop_sleep_ms_simulation } else { fast_runner.loop_sleep_ms_realtrading };
                if sleep_ms > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(sleep_ms.into())).await;
                }
            }
            log_and_println!("{} FastRunnerApTask run() ended", Utc::now().format("%H:%M:%S%.3f"));

            { // keep the mutex lock scope as small as possible
                let mut mark_value_cache = RQ_MARK_VALUE_CACHE.lock_ignore_poison();
                mark_value_cache.stop_quote_stream();
            }

            if let Some(email_to_address) = get_rqcore_config().get("email_gyant") {
                let start = tokio::time::Instant::now();
                if let Err(err) = RqEmail::send_text(email_to_address, "RqCore: FastRunnerPqpTask run() ended", fast_runner.user_log.as_str()).await {
                    log_and_println!("RqEmail::send_text() failed: {}", err);
                } else {
                    log_and_println!("Elapsed Time of RqEmail::send_text(): {:.2}us", start.elapsed().as_secs_f64() * 1_000_000.0);
                }
            }
        })
    }
}
