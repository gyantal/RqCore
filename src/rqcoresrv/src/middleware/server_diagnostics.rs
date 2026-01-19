use actix_web::{get, HttpResponse, Responder, http::header::ContentType};
use chrono::{Utc};
use std::{fmt::Write};
use crate::broker_common::brokers_watcher::RQ_BROKERS_WATCHER;
use crate::{SERVER_APP_START_TIME};

#[get("/serverdiagnostics")]
async fn server_diagnostics() -> impl Responder {
     let mut sb = String::from("<html><body><h1>ServerDiagnostics</h1>");

    // App uptime
    let server_app_start_time = SERVER_APP_START_TIME.get().unwrap();
    let utc_time = Utc::now();
    let time_delta = utc_time.signed_duration_since(*server_app_start_time);
    write!(sb, "<h2>Main.exe</h2><br>WebAppStartTimeUtc: {} ({} days {:02}:{:02} hours ago)<br>", server_app_start_time.format("%Y-%m-%d %H:%M:%S"), time_delta.num_days(), time_delta.num_hours() % 24, time_delta.num_minutes() % 60,).unwrap();

    // RuntimeInfo Section
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    write!(sb, "<h2>RuntimeInfo</h2><br>Logical CPUs: {}<br>Server Workers: {}<br>PID: {}<br>", logical_cpus, logical_cpus, std::process::id()).unwrap();
    // Broker watcher
    write!(sb, "<h2>BrokersWatcher</h2>").unwrap();
    let gateways_guard = RQ_BROKERS_WATCHER.gateways.lock().unwrap();
    let gateways = &*gateways_guard;

    write!(sb, "Total gateways: {}<br>", gateways.len()).unwrap();
    for (idx, gw_arc) in gateways.iter().enumerate() {
        let gw = gw_arc.lock().unwrap();
        write!(sb, "Gateway {} → URL: {} | ClientID: {} | Connected: {}<br>", idx, gw.connection_url, gw.client_id, gw.ib_client.is_some()).unwrap();
    }

    HttpResponse::Ok().content_type(ContentType::html()).body(sb)
}

#[get("/webserver/ping")]
pub async fn webserver_ping() -> impl Responder {
    use chrono::Utc;
    HttpResponse::Ok()
        .content_type("text/html")
        .body(format!("<h3>Pong! {}</h3>", Utc::now().format("%Y-%m-%d %H:%M:%S")))
}