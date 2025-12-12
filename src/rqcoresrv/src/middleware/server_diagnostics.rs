use actix_web::{get, HttpResponse, Responder, http::header::ContentType};
use chrono::{DateTime, Utc};
use std::{fmt::Write,sync::{Arc, OnceLock},};
use crate::broker_common::brokers_watcher::RQ_BROKERS_WATCHER;
use crate::RuntimeInfo;

pub static WEB_APP_START_TIME: OnceLock<DateTime<Utc>> = OnceLock::new();
pub static RUNTIME_INFO: OnceLock<Arc<RuntimeInfo>> = OnceLock::new();

#[get("/serverdiagnostics")]
async fn server_diagnostics() -> impl Responder {
     let mut sb = String::from("<html><body><h1>ServerDiagnostics</h1>");

    // App uptime
    let webapp_start_time = WEB_APP_START_TIME.get().unwrap();
    let utc_time = Utc::now();
    let time_delta = utc_time.signed_duration_since(*webapp_start_time);
    write!(sb, "<h2>Main.exe</h2><br>WebAppStartTimeUtc: {} ({} days {:02}:{:02} hours ago)<br>", webapp_start_time.format("%Y-%m-%d %H:%M:%S"), time_delta.num_days(), time_delta.num_hours() % 24, time_delta.num_minutes() % 60,).unwrap();

    // RuntimeInfo Section
    if let Some(info) = RUNTIME_INFO.get() {
        write!(sb, "<h2>RuntimeInfo</h2><br>Logical CPUs: {}<br>Server Workers: {}<br>PID: {}<br>", info.logical_cpus, info.server_workers, info.pid).unwrap();
    }
    write!(sb, "<h2>BrokersWatcher</h2>").unwrap();
    // Broker watcher
    let gateways_guard = RQ_BROKERS_WATCHER.gateways.lock().unwrap();
    let gateways = &*gateways_guard;

    write!(sb, "Total gateways: {}<br>", gateways.len()).unwrap();

    for (idx, gw_arc) in gateways.iter().enumerate() {
        let gw = gw_arc.lock().unwrap();
        write!(sb, "Gateway {} → URL: {} | ClientID: {} | Connected: {}<br>", idx, gw.connection_url, gw.client_id, gw.ib_client.is_some()).unwrap();
    }

    HttpResponse::Ok().content_type(ContentType::html()).body(sb)
}