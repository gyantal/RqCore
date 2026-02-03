use actix_web::{get, HttpResponse, Responder, http::header::ContentType};
use chrono::{Utc};
use std::{fmt::Write};
use crate::broker_common::brokers_watcher::RQ_BROKERS_WATCHER;
use crate::{SERVER_APP_START_TIME};

#[get("/serverdiagnostics")]
async fn server_diagnostics() -> impl Responder {
     let mut sb = String::from("<html><body><h1>ServerDiagnostics</h1>");

    // App uptime
    let server_app_start_time = match SERVER_APP_START_TIME.get() {
        Some(datetime) => datetime,
        None => {
            log::error!("SERVER_APP_START_TIME not initialized");
            return HttpResponse::InternalServerError().body("Server initialization error");
        }
    };
    let utc_time = Utc::now();
    let time_delta = utc_time.signed_duration_since(*server_app_start_time);
    write!(sb, "<h2>Main.exe</h2><br>WebAppStartTimeUtc: {} ({} days {:02}:{:02} hours ago)<br>", server_app_start_time.format("%Y-%m-%d %H:%M:%S"), time_delta.num_days(), time_delta.num_hours() % 24, time_delta.num_minutes() % 60,).ok();

    // RuntimeInfo Section
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    write!(sb, "<h2>RuntimeInfo</h2><br>Logical CPUs: {}<br>Server Workers: {}<br>PID: {}<br>", logical_cpus, logical_cpus, std::process::id()).ok();
    // Broker watcher
    write!(sb, "<h2>BrokersWatcher</h2>").ok();
    let gateways_guard = match RQ_BROKERS_WATCHER.gateways.lock() {
        Ok(guard) => guard,
        Err(err) => {
            log::error!("RQ_BROKERS_WATCHER.gateways mutex poisoned: {}", err);
            return HttpResponse::InternalServerError().body("Gateways state corrupted");
        }
    };
    let gateways = &*gateways_guard;
    write!(sb, "Total gateways: {}<br>", gateways.len()).ok();

    for (idx, gw_arc) in gateways.iter().enumerate() {
        if let Ok(gw) = gw_arc.lock() {
            write!(sb, "Gateway {} → URL: {} | ClientID: {} | Connected: {}<br>", idx, gw.connection_url, gw.client_id, gw.ib_client.is_some()).ok();
        }
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