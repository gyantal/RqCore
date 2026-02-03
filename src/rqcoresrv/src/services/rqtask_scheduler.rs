use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::{Tz, US::Eastern};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::time as tokio_time;
use std::future::Future;
use std::pin::Pin;

use crate::services::fast_runner::FastRunner;

// ---------- Global ----------
pub static RQ_TASK_SCHEDULER: LazyLock<RqTaskScheduler> = LazyLock::new(|| RqTaskScheduler::new());

// ---------- helpers ----------
fn localtime2future_utc(tz: Tz, target_time_tz: NaiveTime) -> DateTime<Utc> { // target_time_tz (11:59) is local in the tz timezone. Use today or tommorrow, whichever is in the future. Convert to UTC at the end.
    let now_tz = Utc::now().with_timezone(&tz);
    let today_target_tz = now_tz.date_naive().and_time(target_time_tz);
    let next_target_tz = if now_tz.time() < target_time_tz { // if target_time_tz is later today in the future, use that. Otherwise, use tomorrow
        today_target_tz
    } else {
        (now_tz + Duration::days(1)).date_naive().and_time(target_time_tz)
    };
    // Handle DST (ambiguous/invalid) by picking earliest valid local time
    tz.from_local_datetime(&next_target_tz)
        .earliest()
        .unwrap()
        .to_utc()
}

// ---------- Task trait ----------
pub trait RqTask: Send + Sync {
    fn name(&self) -> &str;
    fn get_next_trigger_time(&self) -> DateTime<Utc>;
    fn update_next_trigger_time(&self);
    fn run(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

// ---------- Heartbeat ----------
pub struct HeartbeatTask {
    name: String,
    interval: Duration,
    next_time: Mutex<DateTime<Utc>>,
}

impl HeartbeatTask {
    pub fn new() -> Self {
        HeartbeatTask {
            name: "HeartbeatTask".to_string(),
            interval: Duration::minutes(10),
            next_time: Mutex::new(Utc::now() + Duration::minutes(10)),
        }
    }
}

impl RqTask for HeartbeatTask {
    fn name(&self) -> &str { &self.name }

    fn get_next_trigger_time(&self) -> DateTime<Utc> {
        *self.next_time.lock().unwrap()
    }

    fn update_next_trigger_time(&self) {
        let mut next = self.next_time.lock().unwrap();
        *next = Utc::now() + self.interval;
    }

    fn run(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            println!("HeartbeatTask run has started");
        })
    }
}

// ---------- FastRunner PQP (daily 11:59 ET) ----------
pub struct FastRunnerPqpTask {
    name: String,
    next_time: Mutex<DateTime<Utc>>,
}

impl FastRunnerPqpTask {
    pub fn new() -> Self {
        let next_utc = Self::get_next_trigger_time_impl();
        FastRunnerPqpTask {
            name: "FastRunnerPqpTask".to_string(),
            next_time: Mutex::new(next_utc),
        }
    }

    fn get_next_trigger_time_impl() -> DateTime<Utc> { // we run 3 times daily: 2x Simulation, 1x RealTrading at 11:01 ET, 11:30 ET, 11:59 ET
        let tz = Eastern;
        let targets_tz = [
            // NaiveTime::from_hms_opt(15, 26, 00).unwrap(), // for manual test
            NaiveTime::from_hms_opt(11, 1, 30).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 30).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 30).unwrap(),
        ];

        targets_tz
            .into_iter()
            .map(|t| localtime2future_utc(tz, t))
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
            println!("{} FastRunnerPqpTask run() started", Utc::now().format("%H:%M:%S%.3f"));
            let utc_now = Utc::now().date_naive();
            let is_run_today = utc_now.weekday() == chrono::Weekday::Mon;
            // let is_run_today = true; // run every day for testing. Also use this if PQP day is not Monday and it is run manually.
            if is_run_today {
                println!("FastRunnerPqpTask run() starts the loop");
                let tz_et = Eastern;
                let now_et = Utc::now().with_timezone(&tz_et);
                let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
                let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
                let is_live_trading = (now_et - target_et).num_seconds().abs() < 55; // If current time from 12:00 ET is less than 55 seconds, then set it to true.

                let mut fast_runner = FastRunner::new();
                fast_runner.is_simulation = !is_live_trading;

                let loop_endtime = tokio::time::Instant::now()
                    + if is_live_trading { tokio::time::Duration::from_secs(4 * 60 + 30) } 
                        else { tokio::time::Duration::from_secs(30) };

                // >Example running time at trading:
                // 17:00:02.952 FastRunnerPqpTask run(): Loop iteration (IsSimu:false)
                // Elapsed Time of reqwest.Client.get(): 2,054ms. // SA refreshed the page (high demand), or it couldn't come from RAM cache, so 600ms => 2000ms.
                // 17:00:06.433 FastRunnerPqpTask run() ended
                // it was 2 trades sent. It took 3.5 seconds (including downloading the page (2sec), getting the 2 prices, sending the order)
                // as 2sec was the download URL time, RqCore handles it in 1.5sec with 2 price query and 2 order. So, about 500ms per stock.
                while tokio::time::Instant::now() < loop_endtime { // if the loop runs more than 4 minutes 30 seconds, then finish the loop
                    println!(">*{} FastRunnerPqpTask run(): Loop iteration (IsSimu:{})", Utc::now().format("%H:%M:%S%.3f"), fast_runner.is_simulation);

                    fast_runner.fastrunning_loop_pqp_impl().await;

                    if fast_runner.has_trading_ever_started {
                        println!("FastRunnerPqpTask: Trading has started, exiting the loop.");
                        break;
                    }

                    let sleep_ms = if fast_runner.is_simulation { fast_runner.loop_sleep_ms_simulation } else { fast_runner.loop_sleep_ms_realtrading };
                    if sleep_ms > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(sleep_ms.into())).await;
                    }
                }
            }
            println!("{} FastRunnerPqpTask run() ended", Utc::now().format("%H:%M:%S%.3f"));
        })
    }
}


// ---------- FastRunner AP (daily 11:59 ET) ----------
pub struct FastRunnerApTask {
    name: String,
    next_time: Mutex<DateTime<Utc>>,
}

impl FastRunnerApTask {
    pub fn new() -> Self {
        let next_utc = Self::get_next_trigger_time_impl();
        FastRunnerApTask {
            name: "FastRunnerApTask".to_string(),
            next_time: Mutex::new(next_utc),
        }
    }

    fn get_next_trigger_time_impl() -> DateTime<Utc> { // we run 3 times daily: 2x Simulation, 1x RealTrading at 11:01 ET, 11:30 ET, 11:59 ET
        let tz = Eastern;
        let targets_tz = [
            // NaiveTime::from_hms_opt(15, 26, 10).unwrap(), // for manual test
            NaiveTime::from_hms_opt(11, 5, 40).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 40).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 40).unwrap(),
        ];

        targets_tz
            .into_iter()
            .map(|t| localtime2future_utc(tz, t))
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
            println!("{} FastRunnerApTask run() started", Utc::now().format("%H:%M:%S%.3f"));

            let now_utc = Utc::now().date_naive();
            let virtual_rebalance_date = if now_utc.day() >= 15 { // virtual_rebalance_date as the 1st or 15th of month
                now_utc.with_day(15).unwrap()
            } else {
                now_utc.with_day(1).unwrap()
            };
            let real_rebalance_date = match virtual_rebalance_date.weekday() { // real_rebalance_date as virtual_rebalance_date or the first weekday after it if it falls on a weekend
                chrono::Weekday::Sat => virtual_rebalance_date + chrono::Duration::days(2),
                chrono::Weekday::Sun => virtual_rebalance_date + chrono::Duration::days(1),
                _ => virtual_rebalance_date,
            };

            // Check if today is the real_rebalance_date
            let is_run_today = now_utc == real_rebalance_date;
            // let is_run_today = true; // run every day for testing
            if is_run_today {
                println!("FastRunnerApTask run() starts the loop");
                let tz_et = Eastern;
                let now_et = Utc::now().with_timezone(&tz_et);
                let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
                let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
                let is_live_trading = (now_et - target_et).num_seconds().abs() < 55; // If current time from 12:00 ET is less than 55 seconds, then set it to true.

                let mut fast_runner = FastRunner::new();
                fast_runner.is_simulation = !is_live_trading;

                let loop_endtime = tokio::time::Instant::now()
                    + if is_live_trading { tokio::time::Duration::from_secs(4 * 60 + 30) } // 2025-12-01: AP/Analysis tab published at 12:01:15 ET (late), the AP history was published 30 seconds earlier
                        else { tokio::time::Duration::from_secs(30) };

                while tokio::time::Instant::now() < loop_endtime { // if the loop runs more than 4 minutes 30 seconds, then finish the loop
                    println!(">*{} FastRunnerApTask run(): Loop iteration (IsSimu:{})", Utc::now().format("%H:%M:%S%.3f"), fast_runner.is_simulation);

                    fast_runner.fastrunning_loop_ap_impl().await;

                    if fast_runner.has_trading_ever_started {
                        println!("FastRunnerApTask: Trading has started, exiting the loop.");
                        break;
                    }

                    let sleep_ms = if fast_runner.is_simulation { fast_runner.loop_sleep_ms_simulation } else { fast_runner.loop_sleep_ms_realtrading };
                    if sleep_ms > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(sleep_ms.into())).await;
                    }
                }
            }
            println!("{} FastRunnerApTask run() ended", Utc::now().format("%H:%M:%S%.3f"));
        })
    }
}

// ---------- Scheduler ----------
pub struct RqTaskScheduler {
    tasks: Mutex<Vec<Arc<dyn RqTask>>>,
}

impl RqTaskScheduler {
    pub fn new() -> Self {
        RqTaskScheduler { tasks: Mutex::new(Vec::new()) }
    }

    pub fn schedule_task(&self, task: Arc<dyn RqTask>) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(task);
    }

    pub fn print_next_trigger_times(&self) {
        let tasks = self.tasks.lock().unwrap();
        for t in tasks.iter() {
            println!("{} -> {}", t.name(), t.get_next_trigger_time());
        }
    }

    pub fn start(&self) {
        tokio::spawn(async {
            log::debug!("RqTaskScheduler started");
            loop {
                let now = Utc::now();
                let mut due_tasks: Vec<Arc<dyn RqTask>> = Vec::new();
                {
                    let tasks = RQ_TASK_SCHEDULER.tasks.lock().unwrap();
                    for task in tasks.iter() {
                        let trigger = task.get_next_trigger_time();
                        if trigger <= now {
                            due_tasks.push(task.clone());
                        }
                    }
                }

                // Spawn due tasks as separate async tasks (fire-and-forget, no awaiting);
                for task in due_tasks {
                    let task_clone = task.clone();
                    tokio::spawn(async move {  // spawned method might only starts as this sync thread returns to the tokio runtime at next await point
                        task_clone.run().await;
                    });
                    task.update_next_trigger_time(); // their trigger time is in the past, so update it
                }

                // Recompute soonest
                let mut soonest: Option<DateTime<Utc>> = None;
                {
                    let tasks = RQ_TASK_SCHEDULER.tasks.lock().unwrap();
                    for task in tasks.iter() {
                        let trigger = task.get_next_trigger_time();
                        match soonest {
                            Some(s) if trigger < s => soonest = Some(trigger),
                            None => soonest = Some(trigger),
                            _ => {}
                        }
                    }
                }

                if let Some(s) = soonest {
                    if s > now {
                        let sleep_duration = (s - now).to_std().unwrap_or(std::time::Duration::from_secs(0));
                        tokio_time::sleep(sleep_duration).await;
                    }
                } else {
                    tokio_time::sleep(std::time::Duration::from_secs(60)).await;
                }
            }
        });
    }
}
