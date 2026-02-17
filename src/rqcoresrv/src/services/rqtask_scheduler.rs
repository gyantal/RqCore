use {
    std::{future::Future, pin::Pin, sync::{Arc, LazyLock, Mutex}},
    tokio::time as tokio_time,
    chrono::{DateTime, Duration, Utc},
};

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
