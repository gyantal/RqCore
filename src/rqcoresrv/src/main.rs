use actix_web::{App, HttpServer, Result};
use actix_web::rt::System;
use actix_web::{dev::{ServiceRequest, ServiceResponse, Transform}, Error};
use actix_files::Files;
use std::task::{Context, Poll};
use std::future::{ready, Ready, Future};
use std::pin::Pin;
use std::thread;

// async fn hello() -> Result<HttpResponse> {
//     Ok(HttpResponse::Ok().body("Hello, Actix Web!"))
// }

// Middleware to rewrite path for thetaconite.com requests
pub struct TaconitePathPrefix;

impl<S, B> Transform<S, ServiceRequest> for TaconitePathPrefix
where
    S: actix_web::dev::Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = TaconitePathPrefixMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TaconitePathPrefixMiddleware { service }))
    }
}

pub struct TaconitePathPrefixMiddleware<S> {
    service: S,
}

impl<S, B> actix_web::dev::Service<ServiceRequest> for TaconitePathPrefixMiddleware<S>
where
    S: actix_web::dev::Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let host = req.connection_info().host().to_lowercase();
        if host.contains("thetaconite.com") {
            let orig_path = req.path();
            // Only rewrite if not already prefixed
            if !orig_path.starts_with("/taconite") {
                let new_path = format!("/taconite{}", orig_path);
                let new_uri = actix_web::http::Uri::builder()
                    .path_and_query(new_path)
                    .build()
                    .unwrap();
                req.head_mut().uri = new_uri;
            }
        }
        let fut = self.service.call(req);
        Box::pin(async move { fut.await })
    }
}

// Function to run the Actix Web server
fn actix_websrv_run() {
    thread::spawn(|| {
        // Use a separate Tokio runtime for the server thread
        let sys = System::new(); // actix_web::rt::System
        sys.block_on(async {
            let http_listening_port = 8080;
            HttpServer::new(|| {
                App::new()
                    .wrap(TaconitePathPrefix)
                    .service(Files::new("/", "./static").prefer_utf8(true).index_file("index.html"))
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
        println!("2. Exit gracefully (Avoid Ctrl-^C).");
        std::io::stdout().flush().unwrap(); // Flush to ensure prompt is shown

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                match input.trim() {
                    "1" => {
                        println!("Hello. I am not crashed yet! :)");
                    }
                    "2" => {
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

#[actix_web::main]  // Enables async main with Tokio
async fn main() -> std::io::Result<()> {
    println!("***RqCoreSrv starting..."); // Print startup message

    actix_websrv_run(); // Run the Actix Web server in a separate thread

    display_console_menu();

    Ok(())
}