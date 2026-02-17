use {
    std::{future::Future, pin::Pin, sync::{Arc, LazyLock, Mutex}},
    tokio::time as tokio_time,
    chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc},
    chrono_tz::US::Eastern,
};

use rqcommon::{log_and_println, utils::{rqemail::RqEmail, time::localtimeonly2future_datetime_tz}};

use crate::{get_rqcore_config, robotrader::fast_runner::FastRunner};

// ---------- Global static variables ----------
pub static RQ_TASK_SCHEDULER: LazyLock<RqTaskScheduler> = LazyLock::new(|| RqTaskScheduler::new());

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
            log::info!("HeartbeatTask run has started");
        })
    }
}

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
            NaiveTime::from_hms_opt(11, 1, 30).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 30).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 30).unwrap(),
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
            fast_runner.init();

            let tz_et = Eastern;
            let now_et = Utc::now().with_timezone(&tz_et);
            let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
            let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
            let is_live_trading_based_on_closeto_noon = (now_et - target_et).num_seconds().abs() < 55; // If current time from 12:00 ET is less than 55 seconds, then set it to true.

            fast_runner.is_simulation = !is_live_trading_based_on_closeto_noon;

            if self.is_manual_user_forcerun {
                fast_runner.pqp_is_run_today = true;
                fast_runner.is_simulation = true; // whatever is the calculation, force simulation in this mode.
            }

            if !fast_runner.pqp_is_run_today {
                log_and_println!("Today is not the scheduled day for FastRunnerPqpTask");
                return;
            }

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
                log_and_println!(">*{} FastRunnerPqpTask run(): Loop iteration (IsSimu:{})", Utc::now().format("%H:%M:%S%.3f"), fast_runner.is_simulation);

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
            if let Some(email_to_address) = get_rqcore_config().get("email_gyant") {
                RqEmail::send_text(email_to_address, "RqCore: FastRunnerPqpTask run() ended", fast_runner.user_log.as_str());
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
            NaiveTime::from_hms_opt(11, 5, 40).unwrap(),
            NaiveTime::from_hms_opt(11, 30, 40).unwrap(),
            NaiveTime::from_hms_opt(11, 59, 40).unwrap(),
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
            fast_runner.init();

            let tz_et = Eastern;
            let now_et = Utc::now().with_timezone(&tz_et);
            let target_naive = now_et.date_naive().and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap());
            let target_et = tz_et.from_local_datetime(&target_naive).earliest().unwrap();
            let is_live_trading_based_on_closeto_noon = (now_et - target_et).num_seconds().abs() < 55; // If current time from 12:00 ET is less than 55 seconds, then set it to true.

            fast_runner.is_simulation = !is_live_trading_based_on_closeto_noon;

            if self.is_manual_user_forcerun {
                fast_runner.ap_is_run_today = true;
                fast_runner.is_simulation = true; // whatever is the calculation, force simulation in this mode.
            }

            if !fast_runner.ap_is_run_today {
                log_and_println!("Today is not the scheduled day for FastRunnerApTask");
                return;
            }

            let loop_endtime = tokio::time::Instant::now()
                + if fast_runner.is_simulation { tokio::time::Duration::from_secs(30)}
                    else { tokio::time::Duration::from_secs(4 * 60 + 30) }; // 2025-12-01: AP/Analysis tab published at 12:01:15 ET (late), the AP history was published 30 seconds earlier

            while tokio::time::Instant::now() < loop_endtime { // if the loop runs more than 4 minutes 30 seconds, then finish the loop
                log_and_println!(">*{} FastRunnerApTask run(): Loop iteration (IsSimu:{})", Utc::now().format("%H:%M:%S%.3f"), fast_runner.is_simulation);

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
            if let Some(email_to_address) = get_rqcore_config().get("email_gyant") {
                RqEmail::send_text(email_to_address, "RqCore: FastRunnerApTask run() ended", fast_runner.user_log.as_str());
            }
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
