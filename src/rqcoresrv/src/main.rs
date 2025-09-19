
use std::{thread, sync::Arc, path::Path};
use log;
use spdlog::{prelude::*, sink::{Sink, StdStreamSink, FileSink}, formatter::{pattern, PatternFormatter}};
use time::macros::datetime;
use chrono::Local;
use actix_web::{web, App, HttpServer, rt::System};
use actix_files::Files;
use ibapi::{prelude::*, Client};

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
    spdlog::init_log_crate_proxy().unwrap(); // This proxy forwards log:: macros to spdlog
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

    let file_sink = Arc::new(FileSink::builder()
        .level_filter(LevelFilter::All)
        .path(log_filename)
        .build()?);
    file_sink.set_formatter(Box::new(PatternFormatter::new(pattern!("{month}{day}T{time}.{millisecond}#{tid}|{level}|{logger}|{source}|{payload}{eol}"))));

    let logger = Logger::builder()
        .sink(Arc::new(stdout_sink))
        .sink(file_sink)
        .build()?;
    logger.set_level_filter(LevelFilter::All); // Note: there is the logger level filter, and each sink has its own level filter as well
    spdlog::set_default_logger(Arc::new(logger));

    spdlog::info!("test spdlog::info()");
    error!("test spdlog::error()");
    debug!("test spdlog::debug() 3 + 2 = {}", 5);
    
    log::warn!("test log::warn()");
    log::info!("test log::info()");
    log::debug!("test log::debug()");
    log::trace!("test log::trace() Detailed trace message");
    Ok(())
}

fn is_taconite_domain(ctx: &actix_web::guard::GuardContext) -> bool {
    ctx.head().headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .map(|host| host.to_lowercase().contains("thetaconite.com"))
        .unwrap_or(false)
}

fn actix_websrv_run() {
    thread::spawn(|| {
        // Use a separate Tokio runtime for the server thread
        let sys = System::new(); // actix_web::rt::System
        sys.block_on(async {
            let http_listening_port = 8080;
            HttpServer::new(|| {
                App::new()
                // We can serve many domains, each having its own subfolder in ./static/
                // However, when we rewritten path in a middleware (from /index.html to /taconite/index.html), it was not being used by Actix Files
                // Because the main Actix -Files service is mounted at the root "/" and doesn't know (?) how to handle the "/taconite" prefix. 
                // We need to mount two separate Files services - one for taconite and one for default content
                // fn_guard(is_taconite_domain) is a quick check based on the Host header, so not much overhead
                    .service(
                        web::scope("")
                            .guard(actix_web::guard::fn_guard(is_taconite_domain))
                            .service(Files::new("/", "./static/taconite").prefer_utf8(true).index_file("index.html"))
                    )
                    .service(
                        web::scope("")
                            .guard(actix_web::guard::fn_guard(|ctx| !is_taconite_domain(ctx)))
                            .service(Files::new("/", "./static").prefer_utf8(true).index_file("index.html"))
                    )
            })
            .bind(format!("0.0.0.0:{}", http_listening_port)).unwrap()  // Don't bind to 127.0.0.1 because it only listens to localhost, not external requests to the IP
            .run()
            .await
            .unwrap();
        });
    });
}

fn display_console_menu() {
    use std::io::{self, Write};

    loop {
        println!();
        // TODO: implement the class ColorConsole from C#/sqcommon/utils, because enum colors would be better. And also, that can log-out timestamps as well. 
        // Or probably better:: use fern::colors::{Color, ColoredLevelConfig}; or better find a popular crate for colored console output
        // Actually, I have to implement my own RqConsole anyway, because we need to log to file, or log the timestamps as well
        println!("\x1b[35m----  (type and press Enter)  ----\x1b[0m"); // Print in magenta using ANSI escape code
        println!("1. Say Hello. Don't do anything. Check responsivenes.");
        println!("2. Test IbAPI.");
        println!("3. Exit gracefully (Avoid Ctrl-^C).");
        std::io::stdout().flush().unwrap(); // Flush to ensure prompt is shown

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                match input.trim() {
                    "1" => {
                        println!("Hello. I am not crashed yet! :)");
                    }
                    "2" => {
                        test_ibapi();
                    }
                    "3" => {
                        println!("Exiting gracefully...");
                        break;
                    }
                    _ => {
                        println!("Invalid choice. Please try again.");
                    }
                }
            }
            Err(e) => {
                println!("Input error: {}. Exiting gracefully...", e);
                break;
            }
        }
    }
}

fn test_ibapi() {
    // TODO: this is IbAPI v.1.2.2. When v2.0 comes out, we have to update this code. And use the async version.
    // >Choose async, realtime bars streaming is only available in async. We might want to stream and check 200 tickers at the same time.
    // The sync version just polls 1 snapshot realtime value.
    let connection_url = "34.251.1.119:7303"; // port info is fine here. OK. Temporary anyway, and login is impossible, because there are 2 firewalls with source-IP check: AwsVm, IbTWS
    let client = Client::connect(connection_url, 63).expect("connection to TWS failed!");

    let contract = Contract::stock("AAPL");

    let historical_data = client
        .historical_data(
            &contract,
            Some(datetime!(2023-04-11 20:00 UTC)),
            1.days(),
            HistoricalBarSize::Hour,
            HistoricalWhatToShow::Trades,
            true,
        )
        .expect("historical data request failed");

    println!("start: {:?}, end: {:?}", historical_data.start, historical_data.end);

    for bar in &historical_data.bars {
        println!("{bar:?}");
    }
    // client is dropped at the end of the scope, disconnecting from TWS (checked)
}


#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    init_log().expect("Failed to initialize logging");
    info!("***Starting RqCoreSrv...");

    actix_websrv_run(); // Run the Actix Web server in a separate thread

    display_console_menu();

    log::info!("END RqCoreSrv"); // The OS will clean up the log file handles and flush the file when the process exits
    Ok(())
}