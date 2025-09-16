use actix_web::{web, App, HttpServer};
use actix_web::rt::System;
use actix_files::Files;
use std::thread;

fn is_taconite_domain(ctx: &actix_web::guard::GuardContext) -> bool {
    ctx.head().headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .map(|host| host.to_lowercase().contains("thetaconite.com"))
        .unwrap_or(false)
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