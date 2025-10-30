use std::{thread, sync::{Arc}, path::Path};
use std::io::{self, Write};
use std::env;
use log;
use spdlog::{prelude::*, sink::{Sink, StdStreamSink, FileSink}, formatter::{pattern, PatternFormatter}};
use time::macros::datetime;
use chrono::Local;
use actix_web::{web, App, HttpServer, rt::System};
use actix_files::Files;
use ibapi::{prelude::*, market_data::historical::WhatToShow};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert, ResolvesServerCertUsingSni};
use rustls::sign::CertifiedKey;
use rustls::crypto::aws_lc_rs::sign::any_supported_type;
use std::fs::File;
use std::io::BufReader;
use std::fmt;
use rustls_pemfile;
use rustls::{ServerConfig};

use crate::broker_common::brokers_watcher::RQ_BROKERS_WATCHER;
use crate::services::rqtask_scheduler::{FastRunnerTask, RQ_TASK_SCHEDULER};
use crate::services::rqtask_scheduler::HeartbeatTask;

mod services {
    pub mod rqtask_scheduler;
    pub mod fast_runner;
}

mod broker_common {
    pub mod brokers_watcher;
}

pub fn sensitive_config_folder_path() -> String {
    if env::consts::OS == "windows" { // On windows, use USERDOMAIN, instead of USERNAME, because USERNAME can be the same on multiple machines (e.g. "gyantal" on both GYANTAL-PC and GYANTAL-LAPTOP)
        let userdomain = env::var("USERDOMAIN").expect("Failed to get USERDOMAIN environment variable");
        match userdomain.as_str() {
            "GYANTAL-PC" => "h:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "GYANTAL-LAPTOP" => "h:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "BALAZS-PC" => "h:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "BALAZS-LAPTOP" => "g:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "DAYA-PC" => "g:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "DAYA-LAPTO" => "g:/.shortcut-targets-by-id/0BzxkV1ug5ZxvVmtic1FsNTM5bHM/GDriveHedgeQuant/shared/GitHubRepos/NonCommitedSensitiveData/RqCore/".to_string(),
            "DRCHARMAT-LAPTOP" => "c:/Agy/NonCommitedSensitiveData/RqCore/".to_string(),
            _ => panic!("Windows user name is not recognized. Add your username and folder here!"),
        }
    } else { // Linux and MacOS
        let username = env::var("LOGNAME").expect("Failed to get LOGNAME environment variable"); // when running in "screen -r" session, LOGNAME is set, but USER is not
        format!("/home/{}/RQ/sensitive_data/", username) // e.g. "/home/rquser/RQ/sensitive_data/https_certs";
    }
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

fn actix_websrv_run() {
    thread::spawn(|| {
        // Use a separate Tokio runtime for the server thread
        let sys = System::new(); // actix_web::rt::System to be able to use async in this new OS thread
        sys.block_on(async {
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
            .bind_rustls_0_23(format!("0.0.0.0:{}", https_listening_port), tls_config).unwrap()
            .run()
            .await
            .unwrap();
        });
    });
}

async fn display_console_menu() {
    println!("display_console_menu(): start");

    let mut fast_runner = services::fast_runner::FastRunner::new();

    loop {
        println!();
        // TODO: implement the class ColorConsole from C#/sqcommon/utils, because enum colors would be better. And also, that can log-out timestamps as well. 
        // Or probably better:: use fern::colors::{Color, ColoredLevelConfig}; or better find a popular crate for colored console output
        // Actually, I have to implement my own RqConsole anyway, because we need to log to file, or log the timestamps as well
        println!("\x1b[35m----  (type and press Enter)  ----\x1b[0m"); // Print in magenta using ANSI escape code
        println!("1. Say Hello. Don't do anything. Check responsivenes.");
        println!("2. Test IbAPI: historical data");
        println!("3. Test IbAPI: trade");
        println!("4. FastRunner: Test HttpDownload");
        println!("5. FastRunner loop: Start");
        println!("6. FastRunner loop: Stop");
        println!("8. TaskScheduler: Show next trigger times.");
        println!("9. Exit gracefully (Avoid Ctrl-^C).");
        std::io::stdout().flush().unwrap(); // Flush to ensure prompt is shown

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                match input.trim() {
                    "1" => {
                        println!("Hello. I am not crashed yet! :)");
                    }
                    "2" => {
                        test_ibapi_hist_data().await;
                    }
                    "3" => {
                        test_ibapi_trade().await;
                    }
                    "4" => {
                        fast_runner.test_http_download().await;
                    }
                    "5" => {
                        fast_runner.start_fastrunning_loop().await;
                    }
                    "6" => {
                        fast_runner.stop_fastrunning_loop().await;
                    }
                    "8" => {
                        RQ_TASK_SCHEDULER.print_next_trigger_times();
                    }
                    "9" => {
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

async fn test_ibapi_hist_data() {
    // Use the async (default): Non-blocking client, and not the sync: Blocking client.
    // Choose async, realtime bars streaming is only available in async. We might want to stream and check 200 tickers at the same time.
    // The sync version just polls 1 snapshot realtime value.
    // let connection_url_dcmain = "34.251.1.119:7303"; // port info is fine here. OK. Temporary anyway, and login is impossible, because there are 2 firewalls with source-IP check: AwsVm, IbTWS
    let connection_url_gyantal = "34.251.1.119:7301";
    let client = Client::connect(connection_url_gyantal, 99).await.expect("connection to TWS failed!");
    println!("Successfully connected to TWS");

    let contract = Contract::stock("AAPL").build();

    let historical_data = client
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

async fn test_ibapi_trade() {
    let gateways = RQ_BROKERS_WATCHER.gateways.lock().unwrap();
    let ib_client_guard = gateways[0].lock().unwrap();  // 0 is dcmain, 1 is gyantal
    let ib_client_dcmain = ib_client_guard.ib_client.as_ref().unwrap();

    let contract = Contract::stock("PM").build();

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

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    init_log().expect("Failed to initialize logging");
    info!("***Starting RqCoreSrv...");
    println!("main() start");

    RQ_BROKERS_WATCHER.init().await;

    RQ_TASK_SCHEDULER.schedule_task(Arc::new(HeartbeatTask::new()));
    RQ_TASK_SCHEDULER.schedule_task(Arc::new(FastRunnerTask::new()));
    RQ_TASK_SCHEDULER.start();

    actix_websrv_run();
    display_console_menu().await;

    RQ_BROKERS_WATCHER.exit().await;
    log::info!("END RqCoreSrv"); // The OS will clean up the log file handles and flush the file when the process exits
    Ok(())
}