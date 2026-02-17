use std::{fs::File, io::BufReader, fmt, sync::Arc};
use actix_files::Files;
use actix_web::{cookie::Key, web, App, HttpServer, middleware::{from_fn, Compress, Logger}, dev::{ServerHandle}};
use rustls::{ServerConfig, crypto::aws_lc_rs::sign::any_supported_type, pki_types::{CertificateDer, PrivateKeyDer}, server::{ClientHello, ResolvesServerCert, ResolvesServerCertUsingSni}, sign::CertifiedKey};
use rustls_pemfile;
use actix_identity::IdentityMiddleware;
use actix_session::{storage::CookieSessionStore, config::PersistentSession, SessionMiddleware};

use rqcommon::utils::runningenv::{sensitive_config_folder_path};
use crate::{
    RuntimeInfo,
    middleware::{ browser_cache_control::browser_cache_control_30_days_middleware, user_account, server_diagnostics::{self}, http_request_logger::{self, HTTP_REQUEST_LOGS, HttpRequestLogs, http_request_logger_middleware}},
    webapps::test_websocket::test_ws::test_websocket_middleware,
};

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

// actix's bind_rustls_0_23() returns std::io::Error, so we return general std::error::Error here.
pub fn actix_websrv_run(runtime_info: Arc<RuntimeInfo>, server_workers: usize) -> Result<(actix_web::dev::Server, ServerHandle), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let cookie_encrypt_secret_key = "A key that is long enough (64 bytes) to encrypt the session cookie content"; // any encryption code that is used to encrypt the 'session' cookie content. Minimum 64 bytes.
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
            .wrap(SessionMiddleware::builder(CookieSessionStore::default(), Key::from(cookie_encrypt_secret_key.as_bytes())) // Uses an encrypted cookie to store the entire session.
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
            .service(server_diagnostics::webserver_ping)
            .service(server_diagnostics::server_diagnostics)
            .service(http_request_logger::http_request_activity_log)
            .service(test_websocket_middleware)
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
    .bind(format!("0.0.0.0:{}", http_listening_port))?  // Don't bind to 127.0.0.1 because it only listens to localhost, not external requests to the IP. Returns std::io::error
    .bind_rustls_0_23(format!("0.0.0.0:{}", https_listening_port), tls_config)? // https://127.0.0.1:8443
    .run();

    let handle = server.handle();
    Ok((server, handle))
}
