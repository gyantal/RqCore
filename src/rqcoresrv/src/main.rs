use std::{sync::{Arc, OnceLock}, path::Path};
use std::fs::File;
use std::io::BufReader;
use std::fmt;
use tokio::io::{self, AsyncBufReadExt};
use std::env;
use log;
use spdlog::{prelude::*, sink::{StdStreamSink, FileSink}, formatter::{pattern, PatternFormatter}};
use time::macros::datetime;
use chrono::Local;
use actix_web::{cookie::Key, web, App, Error, HttpServer,body::MessageBody};
use actix_web::dev::ServerHandle;
use actix_web::dev::ServiceResponse;
use actix_web::middleware::{from_fn, Compress, Logger, Next};
use actix_web::http::header::CACHE_CONTROL;
use actix_files::Files;
use ibapi::{prelude::*, market_data::historical::WhatToShow};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert, ResolvesServerCertUsingSni};
use rustls::sign::CertifiedKey;
use rustls::crypto::aws_lc_rs::sign::any_supported_type;
use rustls_pemfile;
use rustls::{ServerConfig};
use chrono::{Utc, DateTime};

use actix_identity::{IdentityMiddleware};
use actix_session::{storage::CookieSessionStore, config::PersistentSession, SessionMiddleware};
use base64::{engine::general_purpose, Engine};

use crate::{broker_common::brokers_watcher::RQ_BROKERS_WATCHER};
use crate::services::rqtask_scheduler::{RQ_TASK_SCHEDULER, HeartbeatTask, FastRunnerPqpTask, FastRunnerApTask};
use crate::middleware::{ user_account, server_diagnostics::{self}, http_request_logger::{self, HTTP_REQUEST_LOGS, HttpRequestLogs, http_request_logger_middleware}};
pub static SERVER_APP_START_TIME: OnceLock<DateTime<Utc>> = OnceLock::new();

// use rqcommon::sensitive_config_folder_path;
use rqcommon::utils::runningenv::sensitive_config_folder_path;

mod services {
    pub mod rqtask_scheduler;
    pub mod fast_runner;
}

mod middleware {
    pub mod user_account;
    pub mod server_diagnostics;
    pub mod http_request_logger;
}

mod broker_common {
    pub mod brokers_watcher;
}

mod test_ws {
    include!("../src_webapps/test_websocket/test_ws.rs"); // import test_ws.rs file from src_webapps folder
}

// SNI (Server Name Indication): the hostname sent by the client. Used for selecting HTTPS cert.
struct SniWithDefaultFallbackResolver {
    inner: ResolvesServerCertUsingSni, // the main SNI resolver
    default_ck: Arc<CertifiedKey>, // default certified key to use when no SNI match
}

impl fmt::Debug for SniWithDefaultFallbackResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SniWithDefault").finish()
    }
}

impl ResolvesServerCert for SniWithDefaultFallbackResolver {
    fn resolve(&self, ch: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.inner.resolve(ch).or_else(|| Some(self.default_ck.clone()))
    }
}

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

    let logger = spdlog::Logger::builder()
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

// Middleware function to add 30-day cache headers
async fn browser_cache_control_30_days_middleware<B>(
    req: actix_web::dev::ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<impl MessageBody>, Error>
where
    B: MessageBody + 'static,
{
    let mut res = next.call(req).await?;
    res.headers_mut().insert(
        CACHE_CONTROL,
        "public, max-age=2592000".parse().unwrap(),
    );
    Ok(res)
}

fn is_taconite_domain(ctx: &actix_web::guard::GuardContext) -> bool {
    // Prefer HTTP/2 URI host or HTTP/2 authority; But fallback to Host headeri if URI host is missing (e.g. in HTTP/1.1)
    let uri_host = ctx.head().uri.host(); // works only in HTTP/2 as HTTPS protocol. (in HTTP/1.1 head().uri.host() is None).
    let host_header = ctx.head().headers().get("host"); // works only in HTTP/1.1, as HTTP protocol
    let authority_header = ctx.head().headers().get(":authority"); // HTTP/2 pseudo-header. CURL doesn't fill it, but Chrome/Edge fills it with "thetaconite.com" (in case the Uri.host() wouldn't work, this could be used)

    let host = uri_host
        .or_else(|| {
            host_header
            .and_then(|h| h.to_str().ok())
        })
        .unwrap_or("");
    
    println!("DEBUG: UriHost='{:?}' HeaderHost='{:?}' HeaderAuthority='{:?}'", uri_host, host_header, authority_header, );
    host.to_lowercase().contains("thetaconite.com")
}

fn actix_websrv_run(runtime_info: Arc<RuntimeInfo>, server_workers: usize) -> std::io::Result<(actix_web::dev::Server, ServerHandle)> {
    let rqcore_config = match user_account::load_rqcore_config() {
    Ok(rq_config) => rq_config,
    Err(err) => {
            log::error!("Config load error: {}", err);
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Server configuration error"));
        }
    };
    let secret_key = Key::from(&general_purpose::STANDARD.decode(&rqcore_config.api_secret_code).expect("Invalid Base64 key"),);
    let runtime_info_for_server = runtime_info;
    HTTP_REQUEST_LOGS.set(Arc::new(HttpRequestLogs::new())).expect("REQUEST_LOGS already initialized");

    let http_listening_port = 8080;
    let https_listening_port = 8443;

    // Load certificates and keys
    fn load_certs(filename: &str) -> Vec<CertificateDer<'static>> {
        let certfile = File::open(filename).expect(&format!("cannot open certificate file {}", filename));
        let mut reader = BufReader::new(certfile);
        rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>().expect(&format!("invalid certificate in file {}", filename))
    }

    fn load_private_key(filename: &str) -> PrivateKeyDer<'static> {
        let keyfile = File::open(filename).expect(&format!("cannot open private key file {}", filename));
        let mut reader = BufReader::new(keyfile);
        rustls_pemfile::private_key(&mut reader).expect(&format!("invalid private key in file {}", filename)).expect(&format!("no private key found in {}", filename))
    }

    let sensitive_config_folder_path = sensitive_config_folder_path();
    let cert_base_path = format!("{}https_certs/", sensitive_config_folder_path);

    let rq_certs = load_certs(&format!("{}rqcore.com/fullchain.pem", cert_base_path));
    let rq_key = load_private_key(&format!("{}rqcore.com/privkey.pem", cert_base_path));
    let rq_signing_key = any_supported_type(&rq_key).expect("unsupported rqcore private key type");
    let rq_certified_key = CertifiedKey::new(rq_certs, rq_signing_key);

    let theta_certs = load_certs(&format!("{}thetaconite.com/fullchain.pem", cert_base_path));
    let theta_key = load_private_key(&format!("{}thetaconite.com/privkey.pem", cert_base_path));
    let theta_signing_key = any_supported_type(&theta_key).expect("unsupported thetaconite private key type");
    let theta_certified_key = CertifiedKey::new(theta_certs, theta_signing_key);

    // Default cert for 'localhost' and IP. Created as: openssl req -x509 -nodes -days 3650 -newkey rsa:2048 -keyout privkey.pem -out fullchain.pem -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,DNS:127.0.0.1"
    let default_certs = load_certs(&format!("{}localhost/fullchain.pem", cert_base_path));
    let default_key = load_private_key(&format!("{}localhost/privkey.pem", cert_base_path));
    let default_signing_key = any_supported_type(&default_key).expect("unsupported default key");
    let default_certified_key = CertifiedKey::new(default_certs, default_signing_key);

    // the SNI (Server Name Indication) hostname sent by the client
    // ResolvesServerCertUsingSni matches DNS hostnames, not IPs, and SNI itself is defined for hostnames (not addresses). 
    // So IP 127.0.0.1 won’t ever hit an entry in that resolver. We need a SniWithDefaultFallbackResolver to provide a default cert for IP connections.
    let mut sni_resolver = ResolvesServerCertUsingSni::new();
    sni_resolver.add("rqcore.com", rq_certified_key.clone()).expect("Invalid DNS name for rqcore.com");
    sni_resolver.add("www.rqcore.com", rq_certified_key.clone()).expect("Invalid DNS name for www.rqcore.com");
    sni_resolver.add("thetaconite.com", theta_certified_key.clone()).expect("Invalid DNS name for thetaconite.com");
    sni_resolver.add("www.thetaconite.com", theta_certified_key.clone()).expect("Invalid DNS name for www.thetaconite.com");
    sni_resolver.add("localhost", default_certified_key.clone()).expect("Invalid localhost DNS name"); // default cert for localhost and IP e.g. 127.0.0.1

    let cert_resolver = Arc::new(SniWithDefaultFallbackResolver {
        inner: sni_resolver,
        default_ck: Arc::new(default_certified_key.clone()), // use the default (for 'localhost') for IP connections when no domain name sent by client
    });

    let tls_config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(cert_resolver);

    // Advertise both http/2 and http/1.1 support. However Actix's bind_rustls_0_23() automatically adds them (and in future versions, it might add http/3 as well). So, don't explicetly add them here.
    // tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    // Test locally the impersonation of multiple domains with different subfolders
    // curl -v --insecure https://localhost:8443
    // curl -v --insecure https://127.0.0.1:8443
    // curl -v --resolve rqcore.com:8443:127.0.0.1 --insecure https://rqcore.com:8443/  
    // curl -v --resolve thetaconite.com:8443:127.0.0.1 --insecure https://thetaconite.com:8443/ 
    // curl -v --resolve thetaconite.com:8080:127.0.0.1 --insecure http://thetaconite.com:8080/

    let server = HttpServer::new(move || {
        // Keep _runtime_info as an example for Dependency Injection:
        // If you ever need runtime_info inside handlers,
        // you can clone info_for_server here and put it into app data.
        let _runtime_info = runtime_info_for_server.clone();
        App::new()
            .wrap(Logger::default())
            .wrap(Compress::default()) // Enable compression (including Brotli when supported by client). We gave up compile time brotli.exe, as it complicates Linux deployment, and we use browser cache control for 30 days, so clients only get pages once per month. Not frequently. So, not much usefulness. And deployment is easier.
            .wrap(from_fn(browser_cache_control_30_days_middleware))
            .wrap(from_fn(http_request_logger_middleware))
            .wrap(IdentityMiddleware::default()) // Enables Identity API; identity is stored inside the session.
            .wrap(SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone()) // Uses an encrypted cookie to store the entire session.
            .session_lifecycle(PersistentSession::default() // Makes the cookie persistent (not deleted when browser closes).
            .session_ttl(time::Duration::days(365))) // Session validity duration (365 days).
            .cookie_secure(true) // Cookie is only sent over HTTPS (required for SameSite=None).
            .cookie_http_only(true) // Cookie is not accessible from JavaScript (XSS protection).
            .cookie_name("session".to_string()) // Name of the session cookie.
            .cookie_same_site(actix_web::cookie::SameSite::None) // Required for Google OAuth redirects; allows cross-site cookies.
            .cookie_domain(None)
            .build())
            .service(user_account::login)
            .service(user_account::google_callback)
            .service(user_account::logout)
            .service(user_account::user_infor)
            .service(user_account::authorized_sample)
            .service(user_account::root_index)
            .service(user_account::webserver_ping)
            .service(server_diagnostics::server_diagnostics)
            .service(http_request_logger::http_request_activity_log)
            .service(test_ws::test_websocket)
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
    .workers(server_workers)
    .bind(format!("0.0.0.0:{}", http_listening_port))?  // Don't bind to 127.0.0.1 because it only listens to localhost, not external requests to the IP
    .bind_rustls_0_23(format!("0.0.0.0:{}", https_listening_port), tls_config)? // https://127.0.0.1:8443
    .run();

    let handle = server.handle();
    Ok((server, handle))
}


async fn console_menu_loop(server_handle: ServerHandle, runtime_info: Arc<RuntimeInfo>) {
    let mut fast_runner = services::fast_runner::FastRunner::new();

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
        println!("51) FastRunner PQP: Test HttpDownload");
        println!("52) FastRunner PQP: Loop Start");
        println!("53) FastRunner PQP: Loop Stop");
        println!("54) FastRunner AP: Test HttpDownload");
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

                        // testing Rust panic crash with Parse()
                        let num_str = "a45.5";
                        
                        let parse_result = num_str.parse::<f64>();
                        match parse_result {
                            Ok(num) => {
                                print!("test my price: {}", num);
                            },
                            Err(e)  => {
                                // print!("Error {}", e);
                                log::error!("Error {}", e); // should go to console + go to logfile
                            }
                        }

                        // if (parse_result.is_err())
                        // {
                        //     log::error!("Error {}", e); // should go to console + go to logfile
                        //     return parse_result.err(); // retur the error to the caller.
                        // }
                        // let num = parse_result.into_ok();

                        // let num1 = num_str.parse::<f64>().unwrap();
                        // print!("test my price: {}", num1);
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
                fast_runner.test_http_download_pqp().await;
            }
            "52" => {
                fast_runner.start_fastrunning_loop_pqp().await;
            }
            "53" => {
                fast_runner.stop_fastrunning_loop_pqp().await;
            },
            "54" => {
                fast_runner.test_http_download_ap().await;
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

struct RuntimeInfo {
    logical_cpus: usize,
    server_workers: usize,
    pid: u32,
}

// To be able to spawn tokio:spawn() worker tasks in the console menu (it cannot be blocking), 
// keep everything inside the Tokio/Actix runtime and make the console menu itself async.
// ! Create new OS threads with thread::spawn() Only VERY rarely for CPU-bound tasks when you don't want to wait for the ThreadPool delegation. 
// We have 2*nCores = 2*12 or 2*16 = 24..32 worker threads on the ThreadPool already. Plenty. They are instantly available.
// Note that RQ_BROKERS_WATCHER's ib-clients are bound to the main tokio runtime that created them.
// So, you will not be able to use RQ_BROKERS_WATCHER's ib-clients in those new OS threads. 
// Although you can create new ib-clients inside the new OS thread with new connectionID. (usually, it is not worth it)
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_log().expect("Failed to initialize logging");
    SERVER_APP_START_TIME.set(Utc::now()).ok();
    
    info!("***Starting RqCoreSrv...");
    println!("main() start");

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
    });

    // Keep server running
    let _server_result = server.await; // this `Result` may be an `Err` variant, which should be handled

    RQ_BROKERS_WATCHER.exit().await;
    log::info!("END RqCoreSrv"); // The OS will clean up the log file handles and flush the file when the process exits
    Ok(())
}
