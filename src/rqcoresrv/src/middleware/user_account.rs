use actix_web::{ get, HttpRequest, HttpResponse, Responder, http::header,};
use actix_web::web::Query;
use actix_identity::Identity;
use actix_session::Session;
use actix_web::{HttpMessage};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use percent_encoding::{percent_encode, percent_decode_str, NON_ALPHANUMERIC};
use std::{path::Path, fs,};

// use rqcommon::sensitive_config_folder_path;
use rqcommon::utils::runningenv::sensitive_config_folder_path;

// Steps to create Google OAuth Client ID for a web app:
// 1. Go to https://console.cloud.google.com (with gya***l1@gmail.com) and create/select a project.
// 2. Enable "Google Identity Services / OAuth 2.0" API under APIs & Services → Library.
// 3. Configure the OAuth consent screen with app name and emails.
// 4. Navigate to APIs & Services → Credentials → Create Credentials → OAuth Client ID.
// 5. Select "Web application" and add Authorized JavaScript Origins and Redirect URIs.
// 6. Save to generate the Client ID and Client Secret.
// 7. Redirect URI must exactly match scheme + host + path used in login callback.

#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize, Debug)]
struct GoogleUser {
    email: String,
    name: String,
}

#[derive(Debug, Clone)]
pub struct RqCoreConfig {
    pub google_client_id: String,
    pub google_client_secret: String,
    pub api_secret_code: String,
}

pub fn load_rqcore_config() -> RqCoreConfig {
    let sensitive_config_folder_path = sensitive_config_folder_path();
    let rqcore_config_path = format!("{}rqcore.config", sensitive_config_folder_path);

    let content = fs::read_to_string(&rqcore_config_path)
        .unwrap_or_else(|_| panic!("Failed to read config file at {}", rqcore_config_path));

    let mut google_client_id = String::new();
    let mut google_client_secret = String::new();
    let mut api_secret_code = String::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "google_client_id" => google_client_id = value.trim().to_string(),
                "google_client_secret" => google_client_secret = value.trim().to_string(),
                "api_secret_code" => api_secret_code = value.trim().to_string(),
                _ => {} // ignore unknown keys
            }
        }
    }

    if google_client_id.is_empty() { panic!("Missing 'google_client_id' in rqcore.config"); }
    if google_client_secret.is_empty() { panic!("Missing 'client_secret' in rqcore.config"); }
    if api_secret_code.is_empty() { panic!("Missing 'api_client_secret' in rqcore.config"); }

    RqCoreConfig { google_client_id, google_client_secret, api_secret_code}
}

fn get_google_redirect_uri(request: &HttpRequest) -> String {
    let conn_info = request.connection_info().clone();
    let scheme = conn_info.scheme().to_string();
    let host = conn_info.host().to_string();
    format!("{scheme}://{host}/useraccount/login/callback")
}

#[get("/useraccount/login")]
pub async fn login(request: HttpRequest, id: Option<Identity>, query: Query<HashMap<String, String>>) -> impl Responder { 
    if id.is_some() {
        let return_url = query.get("returnUrl").cloned().unwrap_or("/".to_string());
        return HttpResponse::Found()
            .append_header(("Location", return_url))
            .finish();
    }
    let rqcore_config = load_rqcore_config();
    let client_id = rqcore_config.google_client_id;
    let return_url = query.get("returnUrl").cloned().unwrap_or("/".to_string());
    let redirect_uri = get_google_redirect_uri(&request);

    let scope = percent_encode(b"https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",NON_ALPHANUMERIC).to_string();
    let state = percent_encode(return_url.as_bytes(), NON_ALPHANUMERIC).to_string();
    let auth_url = format!("https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}", client_id, redirect_uri, scope, state);

    HttpResponse::Found()
        .append_header(("Location", auth_url))
        .finish()
}

#[get("/useraccount/login/callback")]
pub async fn google_callback(request: HttpRequest, query: Query<HashMap<String, String>>, session: Session) -> impl Responder {
    if let Some(code) = query.get("code") {
        let rqcore_config_details = load_rqcore_config();
        let redirect_uri = get_google_redirect_uri(&request);

        let client = Client::new();
        let params = [
            ("code", code.as_str()),
            ("client_id", rqcore_config_details.google_client_id.as_str()),
            ("client_secret", rqcore_config_details.google_client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ];

        let token_resp = client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .expect("Failed to exchange code")
            .json::<TokenResponse>()
            .await
            .expect("Failed to parse token");

        let user_info = client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(&token_resp.access_token)
            .send()
            .await
            .expect("Failed to fetch userinfo")
            .json::<GoogleUser>()
            .await
            .expect("Failed to parse userinfo");

        session.insert("user_email", &user_info.email).unwrap();
        session.insert("user_name", &user_info.name).unwrap();

        Identity::login(&request.extensions(), user_info.email.clone())
        .expect("Failed to create identity");

        let mut redirect_url = "/".to_string();
        // check if the query contains a "state" key
        if let Some(encoded) = query.get("state") {
            redirect_url = percent_decode_str(encoded).decode_utf8().unwrap().into_owned();
        }
        HttpResponse::Found()
            .append_header((header::LOCATION, redirect_url))
            .finish()
    } else {
        HttpResponse::BadRequest().body("Missing 'code' parameter")
    }
}

#[get("/useraccount/logout")]
pub async fn logout(id: Option<Identity>, session: Session) -> impl Responder {
    if let Some(id) = id { id.logout(); }
    session.clear();

    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "text/html"))
        .body("<h3>Logged out</h3> <a href=\"/useraccount/login\">Login</a>")
}

#[get("/useraccount/userinfo")]
pub async fn user_infor(session: Session) -> impl Responder {
    match session.get::<String>("user_email") {
        Ok(Some(email)) => {
            let name = session.get::<String>("user_name")
                .unwrap_or(Some("User".to_string()))
                .unwrap();

            HttpResponse::Ok()
                .insert_header((header::CONTENT_TYPE, "text/html; charset=utf-8"))
                .body(format!("<h2>Hello, {}!</h2><p>Email: {}</p><a href=\"/useraccount/logout\">Logout</a>", name, email))
        }
        _ => HttpResponse::Unauthorized().body("Not logged in"),
    }
}

#[get("/useraccount/authorized_sample")]
pub async fn authorized_sample(session: Session) -> impl Responder {
    let allowed_users = ["gyantal@gmail.com", "gyantal1@gmail.com", "drcharmat@gmail.com", "laszlo.nemeth.hu@gmail.com", "blukucz@gmail.com", "dayakar.kodirekka@gmail.com"];

    match session.get::<String>("user_email") {
        Ok(Some(email)) if allowed_users.contains(&email.as_str()) => {
            HttpResponse::Ok().body(format!("Welcome, authorized user: {}", email))
        }
        Ok(Some(email)) => {
            HttpResponse::Forbidden().body(format!("Access denied: {}", email))
        }
        _ => HttpResponse::Unauthorized().body("Login required"),
    }
}

#[get("/")]
pub async fn root_index(http_req: HttpRequest, id: Option<Identity>, session: Session) -> impl Responder {
    let host = http_req.connection_info().host().to_string();
    let is_logged_in = id.as_ref().is_some_and(|i| i.id().is_ok());
    let is_taconite = host.contains("thetaconite.com");
    println!("Host: {}, is_taconite: {}", http_req.connection_info().host(), is_taconite);
    // 1. Choose which file to serve
    let filename = if is_logged_in { "index.html" } else { "index_nouser.html" };
    let base_folder = if host.contains("thetaconite.com") { "./static/taconite" } else { "./static" }; // Domain-specific folder
    let file_path = Path::new(base_folder).join(filename);
    println!("Serving file: {}", file_path.display());

    // 2. Read the file content
    let mut html = match std::fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(_) => return HttpResponse::NotFound().body("File not found"),
    };

    // 3. If user is logged in give email + logout link
    if id.is_some() {
        if let Ok(Some(email)) = session.get::<String>("user_email") {
            // let user_email = html_escape::encode_text(&email);

            let user_info_html = format!(
                r#"<div style="margin:20px 0; font-weight:bold; z-index:10; color:#2c3e50;">
                    {email} | <a href="/useraccount/logout">Logout</a>
                   </div>"#
            );

            html = html.replace("</body>", &format!("{user_info_html}\n</body>")); // Insert before </body>
        }
    }

    // 4. Serve the modified HTML
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

#[get("/webserver/ping")]
pub async fn webserver_ping() -> impl Responder {
    use chrono::Utc;
    HttpResponse::Ok()
        .content_type("text/html")
        .body(format!("<h3>Pong! {}</h3>", Utc::now().format("%Y-%m-%d %H:%M:%S")))
}