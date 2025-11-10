use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::{Tz, US::Eastern};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::time as tokio_time;
use std::future::Future;
use std::pin::Pin;

use crate::services::fast_runner::FastRunner;

// ---------- Global ----------
pub static RQ_TASK_SCHEDULER: LazyLock<RqTaskScheduler> = LazyLock::new(|| RqTaskScheduler::new());

// ---------- Shared helper ----------
fn next_daily_time_utc(tz: Tz, target_time: NaiveTime) -> DateTime<Utc> {
    let now_tz = Utc::now().with_timezone(&tz);
    let today_target = now_tz.date_naive().and_time(target_time);
    let next_local = if now_tz.time() < target_time {
        today_target
    } else {
        (now_tz + Duration::days(1)).date_naive().and_time(target_time)
    };
    // Handle DST (ambiguous/invalid) by picking earliest valid local time
    tz.from_local_datetime(&next_local)
        .earliest()
        .unwrap()
        .to_utc()
}

// ---------- Task trait ----------
pub trait RqTask: Send + Sync {
    fn name(&self) -> &str;
    fn get_next_trigger_time(&self) -> DateTime<Utc>;
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

    fn run(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let this = self;
        Box::pin(async move {
            println!("HeartbeatTask run has started");
            let mut next = this.next_time.lock().unwrap();
            *next = Utc::now() + this.interval;
        })
    }
}

// ---------- FastRunner (daily 11:59 ET) ----------
pub struct FastRunnerTask {
    name: String,
    next_time: Mutex<DateTime<Utc>>,
}

impl FastRunnerTask {
    pub fn new() -> Self {
        let next_utc = Self::get_next_trigger_time_utc();
        FastRunnerTask {
            name: "FastRunnerTask".to_string(),
            next_time: Mutex::new(next_utc),
        }
    }

    fn get_next_trigger_time_utc() -> DateTime<Utc> { // we run 3 times daily: 2x Simulation, 1x RealTrading at 11:01 ET, 11:30 ET, 11:59 ET
        let tz = Eastern;
        let targets = [
            NaiveTime::from_hms_opt(11, 1, 20).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 20).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 20).unwrap(),
        ];
        // let targets = [
        //     NaiveTime::from_hms_opt(19, 03, 20).unwrap(),
        //     NaiveTime::from_hms_opt(19, 55, 20).unwrap(),
        //     NaiveTime::from_hms_opt(19, 59, 20).unwrap(),
        // ];

        targets
            .into_iter()
            .map(|t| next_daily_time_utc(tz, t))
            .min()                                // pick the earliest UTC time
            .unwrap()
    }
}



impl RqTask for FastRunnerTask {
    fn name(&self) -> &str { &self.name }

    fn get_next_trigger_time(&self) -> DateTime<Utc> {
        *self.next_time.lock().unwrap()
    }

    fn run(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let this = self;
        Box::pin(async move {
            println!("FastRunnerTask run() started");
            let utc_now = Utc::now().date_naive();
            // if utc_now.weekday() == chrono::Weekday::Thu {
            if utc_now.weekday() == chrono::Weekday::Mon {
                println!("FastRunnerTask run() starts the loop");
                let tz_et = Eastern;
                let now_et = Utc::now().with_timezone(&tz_et);
                let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
                let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
                let is_live_trading = (now_et - target_et).num_seconds().abs() < 55; // If current time from 12:00 ET is less than 55 seconds, then set it to true.

                let mut fast_runner = FastRunner::new();
                fast_runner.is_simulation = !is_live_trading;
                // fast_runner.start_fastrunning_loop().await; // don't call this, because that will create a new Fastrunner instance and start its own loop
                

                let loop_endtime = tokio::time::Instant::now()
                    + if is_live_trading {
                        tokio::time::Duration::from_secs(4 * 60 + 30)
                    } else {
                        tokio::time::Duration::from_secs(30)
                    };
                while tokio::time::Instant::now() < loop_endtime { // if the loop runs more than 4 minutes 30 seconds, then finish the loop
                    println!(">* FastRunnerTask run(): Loop iteration (IsSimu:{})", fast_runner.is_simulation);

                    fast_runner.fastrunning_loop_impl().await;

                    if fast_runner.has_trading_ever_started {
                        println!("Trading has started, exiting the loop.");
                        break;
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(if fast_runner.is_simulation { fast_runner.loop_sleep_ms_simulation } else { fast_runner.loop_sleep_ms_realtrading })).await;
                }
                println!("FastRunnerTask run() exits the loop");
            }
            let next_utc = Self::get_next_trigger_time_utc();
            let mut next = this.next_time.lock().unwrap();
            *next = next_utc;
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
            println!("RqTaskScheduler started");
            loop {
                let now = Utc::now();
                let mut due_tasks: Vec<Arc<dyn RqTask>> = Vec::new();
                let mut soonest: Option<DateTime<Utc>> = None;

                {
                    let tasks = RQ_TASK_SCHEDULER.tasks.lock().unwrap();
                    for task in tasks.iter() {
                        let trigger = task.get_next_trigger_time();
                        if trigger <= now {
                            due_tasks.push(task.clone());
                        }
                        match soonest {
                            Some(s) if trigger < s => soonest = Some(trigger),
                            None => soonest = Some(trigger),
                            _ => {}
                        }
                    }
                }

                // Run due tasks asynchronously (await sequentially to keep it simple)
                for task in due_tasks {
                    task.run().await;
                }

                // Recompute soonest after tasks may have updated their next times
                soonest = None;
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
