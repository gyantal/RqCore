use actix_web::{ get, HttpRequest, HttpResponse, Responder, http::header,};
use actix_web::web::Query;
use actix_identity::Identity;
use actix_session::Session;
use actix_web::{HttpMessage};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use percent_encoding::{percent_encode, percent_decode_str, NON_ALPHANUMERIC};

use crate::{get_rqcore_config, main_web::{AUTHORIZED_USERS_LOCK, get_authorized_users}};
// use rqcommon::utils::runningenv::{RqCoreConfig};

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

fn get_google_redirect_uri(request: &HttpRequest) -> String {
    let conn_info = request.connection_info().clone();
    let scheme = conn_info.scheme().to_string();
    let host = conn_info.host().to_string();
    format!("{scheme}://{host}/useraccount/login/callback")
}

fn get_google_oauth_config(http_req: &HttpRequest) -> Result<(String, String), HttpResponse> {
    let host = http_req.connection_info().host().to_string();
    let cfg = get_rqcore_config();

    let (id_key, secret_key) = if host.contains("thetaconite.com") {
        ("taconite_google_client_id", "taconite_google_client_secret")
    } else {
        ("rqcore_google_client_id", "rqcore_google_client_secret")
    };

    let client_id = match cfg.get(id_key) {
        Some(value) => value.to_string(),
        None => {
            log::error!("{} not found in config", id_key);
            return Err(HttpResponse::InternalServerError().body("OAuth config error"));
        }
    };

    let client_secret = match cfg.get(secret_key) {
        Some(value) => value.to_string(),
        None => {
            log::error!("{} not found in config", secret_key);
            return Err(HttpResponse::InternalServerError().body("OAuth config error"));
        }
    };

    Ok((client_id, client_secret))
}

#[get("/useraccount/login")]
pub async fn login(request: HttpRequest, id: Option<Identity>, query: Query<HashMap<String, String>>) -> impl Responder { 
    if id.is_some() {
        let return_url = query.get("returnUrl").cloned().unwrap_or("/".to_string());
        return HttpResponse::Found()
            .append_header(("Location", return_url))
            .finish();
    }

    let (google_client_id, _) = match get_google_oauth_config(&request) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let return_url = query.get("returnUrl").cloned().unwrap_or("/".to_string());
    let redirect_uri = get_google_redirect_uri(&request);

    let scope = percent_encode(b"https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",NON_ALPHANUMERIC).to_string();
    let state = percent_encode(return_url.as_bytes(), NON_ALPHANUMERIC).to_string();
    let auth_url = format!("https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}", google_client_id, redirect_uri, scope, state);

    // After the 'login' or 'logout' http call is processed by the server, the Response header "location: /" will redirect the browser.
    // This 'login' or 'logout' call is the place when we can inject to ask the browser for cache busting for the whole domain: 
    // At login: Clear-Site-Data: "cache". // but we can keep the cookies and storage, because the user is still the same, just logging in. This will ensure that the browser fetches fresh data for the logged-in user, instead of using potentially stale cached data from before login.
    // At logout: Clear-Site-Data: "cache", "cookies", "storage"
    // This tells the browser to remove specific types of stored data associated with the website's origin (e.g., the domain like example.com and its subdomains).
    // HTTPS only. Secure contexts required: Won't work over plain HTTP.
    // This is only needed very rarely, when the user logouts or logs in again (maybe with a different user).
    HttpResponse::Found()
        .append_header((header::CLEAR_SITE_DATA, "\"cache\""))
        .append_header(("Location", auth_url))
        .finish()
}

#[get("/useraccount/login/callback")]
pub async fn google_callback(request: HttpRequest, query: Query<HashMap<String, String>>, session: Session) -> impl Responder {
    let code = match query.get("code") {
        Some(code) => code,
        None => {
            return HttpResponse::BadRequest().content_type("text/plain").body("Missing authorization code");
        }
    };
    let redirect_uri = get_google_redirect_uri(&request);

    let (google_client_id, google_client_secret) = match get_google_oauth_config(&request) {
        Ok(value) => value,
        Err(resp) => return resp,
    };

    let client = Client::new();
    let params = [
        ("code", code.as_str()),
        ("client_id", google_client_id.as_str()),
        ("client_secret", google_client_secret.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];
    let token_resp = match client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        {
            Ok(resp) => match resp.json::<TokenResponse>().await {
                Ok(token) => token,
                Err(e) => {
                    log::error!("Token parse error: {}", e);
                    return HttpResponse::InternalServerError().body("Token parsing failed");
                }
            },
            Err(e) => {
                log::error!("Token exchange failed: {}", e);
                return HttpResponse::InternalServerError().body("Token exchange failed");
            }
        };

    let user_info = match client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&token_resp.access_token)
        .send()
        .await
        { 
            Ok(resp) => match resp.json::<GoogleUser>().await {
                Ok(info) => info,
                Err(e) => {
                    log::error!("Userinfo parse error: {}", e);
                    return HttpResponse::InternalServerError().body("Failed to parse user info");
                }
            },
            Err(e) => {
                log::error!("Failed to fetch userinfo: {}", e);
                return HttpResponse::InternalServerError().body("Failed to fetch user info");
            }
        };

    if let Err(e) = session.insert("user_email", &user_info.email) {
        log::error!("Session insert error: {}", e);
    }

    if let Err(e) = session.insert("user_name", &user_info.name) {
        log::error!("Session insert error: {}", e);
    }

    if let Err(e) = Identity::login(&request.extensions(), user_info.email.clone()) {
        log::error!("Identity login error: {}", e);
        return HttpResponse::InternalServerError().body("Login failed");
    }

    let redirect_url = match query.get("state") {
        Some(encoded) => match percent_decode_str(encoded).decode_utf8() {
            Ok(decoded) => decoded.into_owned(),
            Err(_) => "/".to_string(),
        },
        None => "/".to_string(),
    };

    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}

#[get("/useraccount/logout")]
pub async fn logout(id: Option<Identity>, session: Session) -> impl Responder {
    if let Some(id) = id { id.logout(); }
    session.clear();

    HttpResponse::Found()
        .append_header((header::CLEAR_SITE_DATA, "\"cache\", \"cookies\", \"storage\""))
        .append_header((header::LOCATION, "/"))
        .finish()
}

#[get("/useraccount/userinfo")]
pub async fn user_infor(session: Session) -> impl Responder {
    match session.get::<String>("user_email") {
        Ok(Some(email)) => {
            let name = match session.get::<String>("user_name") {
                Ok(Some(name)) => name,
                Ok(None) => "User".to_string(), // default if not set
                Err(err) => {
                    log::error!("Failed to read 'user_name' from session: {}", err);
                    return HttpResponse::InternalServerError().body("Session error");
                }
            };

            HttpResponse::Ok()
                .insert_header((header::CONTENT_TYPE, "text/html; charset=utf-8"))
                .body(format!("<h2>Hello, {}!</h2><p>Email: {}</p><a href=\"/useraccount/logout\">Logout</a>", name, email))
        }

        Ok(None) => HttpResponse::Unauthorized().body("Not logged in"),

        Err(err) => {
            log::error!("Failed to read 'user_email' from session: {}", err);
            HttpResponse::InternalServerError().body("Session error")
        }
    }
}

#[get("/useraccount/authorized_sample")]
pub async fn authorized_sample(session: Session) -> impl Responder {
    // let auth_users = match AUTHORIZED_USERS_LOCK.get() {
    //     Some(users) => users,
    //     None => return  HttpResponse::InternalServerError().body("Server configuration error")
    // };

    let auth_users = get_authorized_users();

    match session.get::<String>("user_email") {
        Ok(Some(email)) if auth_users.contains(&email) => {
            HttpResponse::Ok().body(format!("Welcome, authorized user: {}", email))
        }
        Ok(Some(email)) => {
            HttpResponse::Forbidden().body(format!("Access denied: {}", email))
        }
        _ => HttpResponse::Unauthorized().body("Login required"),
    }
}

const RQCORE_INDEX: &str = include_str!("../../static/index.html"); // compile time operation. Read the file and represent it as a const String. Increases the EXE size. Index.html changes need recompilation. But that is fine.
const RQCORE_NOUSER: &str = include_str!("../../static/index_nouser.html");
const TACONITE_INDEX: &str = include_str!("../../static/taconite/index.html");
const TACONITE_NOUSER: &str = include_str!("../../static/taconite/index_nouser.html");
const ALLDOMAIN_USER_UNAUTHORIZED_INDEX: &str = r#"You are logged in as {email}, but your user is not <b>authorized</b>.<p>Please logout and login with another user. <a href="/useraccount/logout">Logout</a></p>"#;

#[get("/")] // Without declaring it, this is also called for "/index.html", which is a standard practice.
pub async fn root_index(http_req: HttpRequest, id: Option<Identity>, session: Session) -> impl Responder {
    let host = http_req.connection_info().host().to_string();
    let is_logged_in = id.as_ref().is_some_and(|i| i.id().is_ok());
    let is_taconite = host.contains("thetaconite.com");
    println!("Host: {}, is_taconite: {}", http_req.connection_info().host(), is_taconite);

    // 1. Choose which file to serve
    let (index, index_nouser) = if is_taconite {(TACONITE_INDEX, TACONITE_NOUSER)} else {(RQCORE_INDEX, RQCORE_NOUSER)};
    if !is_logged_in {
        return HttpResponse::Ok().insert_header((header::CONTENT_TYPE, "text/html; charset=utf-8")).body(index_nouser);
    }
    // 2. Get the email
    let email =  match session.get::<String>("user_email") {
        Ok(Some(email)) => email,
        Ok(None) => {
            log::warn!("Email missing in session");
            return HttpResponse::Unauthorized().body("Login required");
        }
        Err(err) => {
            log::error!("Session error: {}", err);
            return HttpResponse::InternalServerError().body("Session error");
        }
    };
    // 3. Get the authorized users
    let auth_users = match AUTHORIZED_USERS_LOCK.get() {
        Some(users) => users,
        None => return HttpResponse::InternalServerError().body("Server configuration error"),
    };
    // 4. Serve the modified HTML
    let html = if auth_users.contains(&email) {
        index.replace("{{USER_EMAIL}}", &email)
    } else {
        ALLDOMAIN_USER_UNAUTHORIZED_INDEX.replace("{email}", &email)
    };

    HttpResponse::Ok().insert_header((header::CONTENT_TYPE, "text/html; charset=utf-8")).body(html)
}