use std::{sync::{Arc, OnceLock}, path::Path, env, collections::HashMap};
use tokio::io::{self, AsyncBufReadExt};
use log;
use spdlog::{prelude::*, sink::{StdStreamSink, FileSink}, formatter::{pattern, PatternFormatter}};
use time::macros::datetime;
use chrono::{Local, Utc, DateTime};
use actix_web::dev::ServerHandle;
use ibapi::{prelude::*, market_data::historical::WhatToShow};

use rqcommon::utils::runningenv::{load_rqcore_config, RqCoreConfig}; // no need of mod rqcommon, broker-common as that is in Cargo.toml as a dependency.
use broker_common::brokers_watcher::RQ_BROKERS_WATCHER;

// All compile target *.rs files in all folders should be mentioned as modules somehow.
// That is the way how only main.rs is compiled by 'cargo build', and that imports all the other .rs files as modules.
// If we don't mention them here as 'mod', they won't be compiled at all,
// and we will get 'cannot find module' error when using them in main.rs or main_web.rs
// *.rs files can be mentioned here one by one in main.rs, or in mod.rs of the specific subfolders (more localized there, less cluttering here if there are many *.rs files in the subfolder).
mod middleware; // refers ./middleware/mod.rs (that refers to many other *.rs files)
// no 'use crate::middleware' here, because main_web.rs uses those, and we refer to them there
mod services; // refers ./services/mod.rs
mod webapps; // refers ./webapps/mod.rs
// no 'use crate::webapps' here, because main_web.rs uses those, and we refer to them there
mod main_web; // refers main_web.rs as a module

use crate::{
    services::rqtask_scheduler::{RQ_TASK_SCHEDULER, HeartbeatTask, FastRunnerPqpTask, FastRunnerApTask, RqTask},
    main_web::actix_websrv_run,
};

// ---------- Global static variables ----------
pub static SERVER_APP_START_TIME: OnceLock<DateTime<Utc>> = OnceLock::new();

// Maybe this is the best way to handle global static. With a get_rqcore_config() supplier function, rather than accessing the global static variable directly.
// because RQCORE_CONFIG_LOCK.get() returns an Option<T>. Even though we 'know' that it cannot be None, because we initialized it,
// Rust simply doesn't believe, doesn't rely on coder's judgement. So, RQCORE_CONFIG_LOCK.get(), we have to do 'match' function for Some, None.
// Which makes the code much uglier, than just using an get_rqcore_config() supplier function (that performs the 'match' internally))
// Option 1*: get_rqcore_config() supplier function:
// "let google_api_secret= match get_rqcore_config().get("google_api_secret_code") {"
// Option 2: use RQCORE_CONFIG_LOCK directly. And flatten the double nested IF scopes by chaining the Options using and_then()
// let google_api_secret= match RQCORE_CONFIG_LOCK.get().and_then(|rq_config| rq_config.get("google_api_secret_code")) {
// Both can be used, but Option1 is more readable.
pub static RQCORE_CONFIG_LOCK: OnceLock<RqCoreConfig> = OnceLock::new();

pub fn get_rqcore_config() -> &'static RqCoreConfig {
    RQCORE_CONFIG_LOCK.get_or_init(|| {
        match load_rqcore_config() {
            Ok(cfg) => cfg,
            Err(err) => {
                log::error!("RqCore config not loaded: {}", err);
                HashMap::new()
            }
        }
    })
}

// ---------- Class/struct definitions ----------
struct RuntimeInfo {
    logical_cpus: usize,
    server_workers: usize,
    pid: u32,
}

// ---------- Helpers ----------

fn init_log() -> Result<(), Box<dyn std::error::Error>> {
    // Check if Cargo.toml exists in the current directory, before creating log file in ../../logs/
    if !Path::new("Cargo.toml").exists() {
        eprintln!("Error: Cargo.toml not found in current directory. We assume this to find the relative ../../logs folder. Run app from the project root.");
        panic!("Cargo.toml not found in current directory");
    }
    // Generate log filename with current date. E.g. rqcoresrv.2025-10-05.sqlog
    let log_filename = format!("../../logs/rqcoresrv.{}.sqlog", Local::now().format("%Y-%m-%d"));

    // To enable that spdlog-rs processes the 'log crate' macros (info(),error(), trace()...) from other libs, we need to do 3 things:
    // 1. Cargo.toml: switch on "Log" compatibility: spdlog-rs = {version = "0.4.3", features = ["source-location", "log"]}
    // 2. init_log_crate_proxy();
    // 3. log::set_max_level(), otherwise 'Log crate' will not send all logs to spdlog's proxy
    spdlog::init_log_crate_proxy()?; // This proxy forwards log:: macros to spdlog
    // Create a filter builder and parse directives (e.g., from RUST_LOG env var). Better to use env var, so we can change log levels without recompiling.
    // set RUST_LOG= "warn,rqcoresrv=info,ibapi=info" // this sets all crates to warn, but the main crate to info.
    // If RUST_LOG is not set, the filter that matches nothing. This means no logs are emitted. 
    let log_crate_filter = env_filter::Builder::from_env("RUST_LOG").build();  // Or use .new("warn,rqcoresrv=info,ibapi=info") for hardcoded
    spdlog::log_crate_proxy().set_filter(Some(log_crate_filter)); // this only filters, the log_crate's log::info!(), but the spdlog::info!() are not filtered here.

    // Set the max level for the 'log crate' (this is required to allow messages through)
    log::set_max_level(log::LevelFilter::Trace); // This was the problem why spdlog::info!() appeared, but log::info!() didn't. 

    // Build a new logger from scratch to avoid duplication issues with the 2 default sinks
    // The default logger has 2 sinks: stderr for warn and above, and stdout for info and below. So, there is no duplicated messages by default, even though in Terminal, both stdout and stderr are shown in console.
    // For us, 1 console sink is enough: stdout for warn and above. And a file sink for all logs. We don't use stderr at all.
    let stdout_sink = StdStreamSink::builder()
            .stdout()
            .level_filter(LevelFilter::MoreSevereEqual(Level::Warn))
            .build()
            .unwrap();
    // # Appendix: Full List of Built-in Patterns here: https://github.com/SpriteOvO/spdlog-rs/blob/main/spdlog/src/formatter/pattern_formatter/mod.rs
    stdout_sink.set_formatter(Box::new(PatternFormatter::new(pattern!("{month}{day}T{time}.{millisecond}#{tid}|{level}|{module_path}|{payload}{eol}"))));

    let file_sink =FileSink::builder()
        .level_filter(LevelFilter::All)
        .path(log_filename)
        .build()?;
    file_sink.set_formatter(Box::new(PatternFormatter::new(pattern!("{month}{day}T{time}.{millisecond}#{tid}|{level}|{logger}|{source}|{payload}{eol}"))));

    let logger = spdlog::Logger::builder()
        .sink(Arc::new(stdout_sink))
        .sink( Arc::new(file_sink))
        .build()?;
    logger.set_level_filter(LevelFilter::All); // Note: there is the logger level filter, and each sink has its own level filter as well
    spdlog::set_default_logger(Arc::new(logger));

    // spdlog::info!("test spdlog::info()"); // spdlog::info!() goes through, even though RUST_LOG is not set (because that controls the log::info!())
    // spdlog::error!("test spdlog::error()");
    // spdlog::debug!("test spdlog::debug() 3 + 2 = {}", 5);
    
    // log::warn!("test log::warn()");
    // log::info!("test log::info()");
    // log::debug!("test log::debug()");
    // log::trace!("test log::trace() Detailed trace message");
    Ok(())
}

async fn console_menu_loop(server_handle: ServerHandle, runtime_info: Arc<RuntimeInfo>) {
    let stdin = io::stdin();
    let mut lines = io::BufReader::new(stdin).lines();

    loop {
        println!();
        // TODO: implement the class ColorConsole from C#/sqcommon/utils, because enum colors would be better. And also, that can log-out timestamps as well.
        // Or probably better:: use fern::colors::{Color, ColoredLevelConfig}; or better find a popular crate for colored console output
        // Actually, I have to implement my own RqConsole anyway, because we need to log to file, or log the timestamps as well
        println!("\x1b[35m----  (type and press Enter)  ----\x1b[0m"); // Print in magenta using ANSI escape code
        println!("1) Say Hello. Don't do anything. Check responsivenes.");
        println!("2) Show runtime info");
        println!("3) TaskScheduler: Show next trigger times.");
        println!("41) Test: tokio::spawn() background async task in main runtime");
        println!("42) Test IbAPI (gyantal): historical data");
        println!("43) Test IbAPI (dcmain): realtime bars");
        println!("51) FastRunner PQP: test only HttpDownload");
        println!("52) FastRunner AP: test only HttpDownload");
        println!("53) FastRunnerTask PQP: Forcerun trade simulation");
        println!("54) FastRunnerTask AP: Forcerun trade simulation (getprice() hangs OTH)");
        println!("9) Stop server and exit gracefully (Avoid Ctrl-^C).");
        print!("Choice: ");
        // flush stdout (small blocking is fine here)
        use std::io::Write;
        let _ = std::io::stdout().flush(); // Flush to ensure prompt is shown

        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            _ => {
                println!("stdin closed, exiting menu.");
                break;
            }
        };

        match line.trim() {
            "1" => {
                println!("Hello. I am not crashed yet! :)");
            }
            "2" => {
                print_runtime_info(&runtime_info);
            },
            "3" => {
                RQ_TASK_SCHEDULER.print_next_trigger_times();
            },
            "41" => {
                println!("Spawning background async task...");
                tokio::spawn(async {
                    loop {
                        println!("[task] running...");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                });
            }
            "42" => {
                test_ibapi_hist_data().await;
            }
            "43" => {
                test_ibapi_realtime_bars().await;
            }
            "51" => {
                let mut fast_runner = services::fast_runner::FastRunner::new();
                fast_runner.init();
                fast_runner.test_http_download_pqp().await;
            }
            "52" => {
                let mut fast_runner = services::fast_runner::FastRunner::new();
                fast_runner.init();
                fast_runner.test_http_download_ap().await;
            }
            "53" => {
                let mut task = FastRunnerPqpTask::new();
                task.is_manual_user_forcerun = true;
                task.run().await;
            },
            "54" => {
                let mut task = FastRunnerApTask::new();
                task.is_manual_user_forcerun = true;
                task.run().await;
            }
            "9" => {
                println!("Stopping server...");
                server_handle.stop(false).await;
                break;
            }
            other => {
                println!("Unknown choice: {other}");
            }
        }
    }
}

fn print_runtime_info(info: &RuntimeInfo) {
    println!();
    println!("=== Runtime Info ===");
    println!("Process ID:         {}", info.pid);
    println!("Logical CPUs:       {}", info.logical_cpus);
    println!("Actix workers:      {}", info.server_workers);
    println!("(Tokio worker pool ≈ logical CPUs, unless you customized it)");
}


async fn test_ibapi_hist_data() {
    // Use the async (default): Non-blocking client, and not the sync: Blocking client.
    // Choose async, realtime bars streaming is only available in async. We might want to stream and check 200 tickers at the same time.
    // The sync version just polls 1 snapshot realtime value.
    // let connection_url_dcmain = "34.251.1.119:7303"; // port info is fine here. OK. Temporary anyway, and login is impossible, because there are 2 firewalls with source-IP check: AwsVm, IbTWS
    // let connection_url_gyantal = "34.251.1.119:7301";
    // let client = Client::connect(connection_url_gyantal, 99).await.expect("connection to TWS failed!");
    let ib_client_gyantal = { // 0 is dcmain, 1 is gyantal
        let gateways = RQ_BROKERS_WATCHER.gateways.lock().unwrap();
        gateways[1]
            .lock()
            .unwrap()
            .ib_client
            .as_ref()
            .cloned()
            .expect("ib_client is not initialized")
    };
    println!("Successfully connected to TWS");

    let contract = Contract::stock("AAPL").build();

    let historical_data = ib_client_gyantal
        .historical_data(
            &contract,
            Some(datetime!(2023-04-11 20:00 UTC)),
            1.days(),
            HistoricalBarSize::Hour,
            Some(WhatToShow::Trades),
            TradingHours::Regular,
        ).await
        .expect("historical data request failed");

    println!("start: {:?}, end: {:?}", historical_data.start, historical_data.end);

    for bar in &historical_data.bars {
        println!("{bar:?}");
    }
    // client is dropped at the end of the scope, disconnecting from TWS (checked)
}

async fn test_ibapi_realtime_bars() {
    let contract = Contract::stock("PM").build();

    let ib_client_dcmain = { // 0 is dcmain, 1 is gyantal
            let gateways = RQ_BROKERS_WATCHER.gateways.lock().unwrap();
            gateways[0]
                .lock()
                .unwrap()
                .ib_client
                .as_ref()
                .cloned()
                .expect("ib_client is not initialized")
        };

    // ib_client_gyantal: Error: Parse(5, "Invalid Real-time Query:No market data permissions for NYSE STK. Requested market data requires additional subscription for API. See link in 'Market Data Connections' dialog for more details."
    let mut subscription = ib_client_dcmain
        .realtime_bars(&contract, RealtimeBarSize::Sec5, RealtimeWhatToShow::Trades, TradingHours::Regular).await
        .expect("realtime bars request failed!");

    while let Some(bar_result) = subscription.next().await {
        match bar_result {
            Ok(bar) => println!("{bar:?}"),
            Err(e) => eprintln!("Error: {e:?}"),
        }
        break; // just 1 bar for testing, otherwise it would block here forever
    }

    // This will do a real trade, so commented out for safety. Just comment it back in when you want to test.
    // let ib_client_gyantal = brokers_watcher.gateways[1].ib_client.as_ref().unwrap();
    // let order_id = ib_client_gyantal.order(&contract)
    //     .buy(1)
    //     .market()
    //     .submit()
    //     .await
    //     .expect("order submission failed!");
    // println!("Order submitted with ID: {}", order_id);
}

// To be able to spawn tokio:spawn() worker tasks in the console menu (it cannot be blocking), 
// keep everything inside the Tokio/Actix runtime and make the console menu itself async.
// ! Create new OS threads with thread::spawn() Only VERY rarely for CPU-bound tasks when you don't want to wait for the ThreadPool delegation. 
// We have 2*nCores = 2*12 or 2*16 = 24..32 worker threads on the ThreadPool already. Plenty. They are instantly available.
// Note that RQ_BROKERS_WATCHER's ib-clients are bound to the main tokio runtime that created them.
// So, you will not be able to use RQ_BROKERS_WATCHER's ib-clients in those new OS threads. 
// Although you can create new ib-clients inside the new OS thread with new connectionID. (usually, it is not worth it)
#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> { // actix's bind_rustls_0_23() returns std::io::Error
    init_log().expect("Failed to initialize logging"); // if init_log() fails, we are happy to panic crash
    SERVER_APP_START_TIME.set(Utc::now()).ok();
    
    spdlog::info!("***Starting RqCoreSrv...");  // spdlog::info!() goes through, even though RUST_LOG is not set (because that controls the log::info!())
    // Initialize the global variable RqCoreConfig now (only once), before parallel threads start to use it.
    spdlog::info!("RqCore config loaded: {} entries", get_rqcore_config().len());

    RQ_BROKERS_WATCHER.init().await;

    RQ_TASK_SCHEDULER.schedule_task(Arc::new(HeartbeatTask::new()));
    // In the future FastRunner tasks will be scheduled on Linux server only.
    if env::consts::OS == "windows" { // 2025-12-01: only schedule FastRunner tasks on GYANTAL-PC and GYANTAL-LAPTOP (to avoid other developers' machines running them)
        let userdomain = env::var("USERDOMAIN").expect("Failed to get USERDOMAIN environment variable");
        if (userdomain.as_str() == "GYANTAL-PC") || (userdomain.as_str() == "GYANTAL-LAPTOP") {
            RQ_TASK_SCHEDULER.schedule_task(Arc::new(FastRunnerPqpTask::new()));
            RQ_TASK_SCHEDULER.schedule_task(Arc::new(FastRunnerApTask::new()));
        }
    }
    RQ_TASK_SCHEDULER.start();

    // Detect CPU count
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // You can choose a different worker count if you like; on a 12 Core / 24 Thread machine, 24 workers is probably overkill, but fine.
    let server_workers = logical_cpus;

    let runtime_info = Arc::new(RuntimeInfo {
        logical_cpus,
        server_workers,
        pid: std::process::id(),
    });

    let (server, server_handle) = actix_websrv_run(runtime_info.clone(), server_workers)?;

    // Spawn async console menu INSIDE the Tokio runtime
    tokio::spawn(async move {
        console_menu_loop(server_handle, runtime_info).await;
        println!("console_menu_loop() end");
    });

    // Keep server running
    let _server_result = server.await; // this `Result` may be an `Err` variant, which should be handled

    RQ_BROKERS_WATCHER.exit().await;
    log::info!("END RqCoreSrv"); // The OS will clean up the log file handles and flush the file when the process exits
    Ok(())
}
