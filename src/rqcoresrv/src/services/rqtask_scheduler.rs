use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::{Tz, US::Eastern};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time;

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
    fn run(&self);
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
            interval: Duration::minutes(1),
            next_time: Mutex::new(Utc::now() + Duration::minutes(1)),
        }
    }
}

impl RqTask for HeartbeatTask {
    fn name(&self) -> &str { &self.name }

    fn get_next_trigger_time(&self) -> DateTime<Utc> {
        *self.next_time.lock().unwrap()
    }

    fn run(&self) {
        println!("HeartbeatTask run has started");
        let mut next = self.next_time.lock().unwrap();
        *next = Utc::now() + self.interval;
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

    // fn get_next_trigger_time_utc() -> DateTime<Utc> {
    //     let target_time = NaiveTime::from_hms_opt(11, 59, 10).unwrap();
    //     let next_utc = next_daily_time_utc(Eastern, target_time);
    //     next_utc
    // }

    fn get_next_trigger_time_utc() -> DateTime<Utc> { // we run 3 times daily: 2x Simulation, 1x RealTrading at 11:01 ET, 11:30 ET, 11:59 ET
        let tz = Eastern;
        // let targets = [
        //     NaiveTime::from_hms_opt(11, 1, 10).unwrap(),
        //     NaiveTime::from_hms_opt(11, 30, 10).unwrap(),
        //     NaiveTime::from_hms_opt(11, 59, 10).unwrap(),
        // ];
        let targets = [
            NaiveTime::from_hms_opt(12, 47, 10).unwrap(),
            NaiveTime::from_hms_opt(12, 48, 10).unwrap(),
            NaiveTime::from_hms_opt(12, 49, 10).unwrap(),
        ];

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

    fn run(&self) {
        println!("FastRunnerTask run has started");
        let utc_now = Utc::now().date_naive();
        if utc_now.weekday() == chrono::Weekday::Mon { // Only run on Mondays
            println!("FastRunnerTask is triggering FastRunner loop");
            let mut fast_runner = FastRunner::new();
            // fast_runner.start_fastrunning_loop(brokers_watcher_guard).await;
            
        }
        // TODO: 1. Async problem. 2. is_simulation = true/false 3. Loop should close after 4 minutes

        let mut fast_runner2 = FastRunner::new(); // fake another instance, because self cannot be used, because it will be out of scope after this function returns
        fast_runner2.is_simulation = true; // set this based on when it was triggered
        loop {
            println!(">* Loop iteration");

            // fast_runner2.fastrunning_loop_impl().await;

            if fast_runner2.has_trading_ever_started {
                println!("Trading has started, exiting the loop.");
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(if fast_runner2.is_simulation { fast_runner2.loop_sleep_ms_simulation } else { fast_runner2.loop_sleep_ms_realtrading })).await;
        }

        let next_utc = Self::get_next_trigger_time_utc();
        let mut next = self.next_time.lock().unwrap();
        *next = next_utc;
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
        thread::spawn(|| {
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

                for task in due_tasks {
                    task.run(); // TODO: Consider running in separate threads if tasks are long-running, because this will block the scheduler loop and delay other tasks.
                }

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
                        let sleep_duration = (s - now).to_std().unwrap_or(time::Duration::from_secs(0));
                        thread::sleep(sleep_duration);
                    }
                } else {
                    thread::sleep(time::Duration::from_secs(60));
                }
            }
        });
    }
}
