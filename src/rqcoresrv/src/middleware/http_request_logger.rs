use actix_session::SessionExt;
use actix_web::{body::MessageBody, dev::{ServiceRequest, ServiceResponse}, get, middleware::Next, Error, HttpResponse,};
use chrono::{DateTime, Utc};
use std::{collections::VecDeque, fmt::Write, net::IpAddr, sync::Mutex,};
use crate::HTTP_REQUEST_LOGS;

#[derive(Debug, Clone)]
pub struct HttpRequestLog {
    pub start_time: DateTime<Utc>,
    pub is_error: bool,
    pub is_https: bool,
    pub method: String,
    pub path: String,
    pub query_string: String,
    pub client_ip: IpAddr,
    pub client_user_email: String,
    pub status_code: u16,
    pub total_milliseconds: f64,
}

#[derive(Debug)]
pub struct HttpRequestLogs {
    pub http_request_logs: Mutex<VecDeque<HttpRequestLog>>,
}

impl HttpRequestLogs {
    // Creates a new request log store with a fixed capacity.
    pub fn new() -> Self {
        Self {
            http_request_logs: Mutex::new(VecDeque::with_capacity(50)),
        }
    }
    // Formatting logs as html string
    pub fn http_request_activity(&self) -> String {
        let logs: Vec<HttpRequestLog> = {
            let guard = self.http_request_logs.lock().unwrap();
            guard.iter().cloned().collect()
        };

        let mut sb = String::new();

        for log in logs.iter().rev() {
            let _ = write!(
                sb,
                "{}#{}{} {} '{}' from {} (u: {}) ret: {} in {:.2}ms<br/>",
                log.start_time.format("%H:%M:%S%.3f"),
                if log.is_error { "ERROR in " } else { "" },
                if log.is_https { "HTTPS" } else { "HTTP" },
                log.method,
                if log.query_string.is_empty() {
                    log.path.clone()
                } else {
                    format!("{}{}", log.path, log.query_string)
                },
                log.client_ip,
                log.client_user_email,
                log.status_code,
                log.total_milliseconds
            );
        }
        sb
    }
}

// Logs Http request and response details
pub async fn http_request_logger_middleware<B>(
    service_req: ServiceRequest,
    next: Next<B>
) -> Result<ServiceResponse<B>, Error>
where
    B: MessageBody + 'static,
{
    let start = std::time::Instant::now();
    let start_time = Utc::now();

    let is_https = service_req.connection_info().scheme() == "https";
    let method = service_req.method().to_string();
    let path = service_req.path().to_string();
    let query_string = service_req.query_string().to_string();

    let client_ip = service_req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("0.0.0.0")
        .parse::<IpAddr>()
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));

     let user_email = service_req
        .get_session()
        .get::<String>("user_email")
        .unwrap_or(None)
        .unwrap_or_default();
    // Execute next service
    let res = next.call(service_req).await?;

    let status_code = res.status().as_u16();
    let is_error = status_code >= 500;
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

    if let Some(store) = HTTP_REQUEST_LOGS.get() {
        let mut logs = store.http_request_logs.lock().unwrap();
        if logs.len() >= 50 {
            logs.pop_front();
        }

        logs.push_back(HttpRequestLog {
            start_time,
            is_error,
            is_https,
            method,
            path,
            query_string,
            client_ip,
            client_user_email: user_email,
            status_code,
            total_milliseconds: duration_ms,
        });
    }
    Ok(res)
}

#[get("/httprequestactivitylog")]
pub async fn http_request_activity_log() -> HttpResponse {
    let logs_html = HTTP_REQUEST_LOGS.get().unwrap().http_request_activity();
    let full_html = format!("<html><body><h1>HttpRequests Activity Log</h1>{}</body></html>", logs_html );
    HttpResponse::Ok().content_type("text/html").body(full_html)
}
