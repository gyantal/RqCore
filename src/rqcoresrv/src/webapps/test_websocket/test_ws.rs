use actix_web::{get, HttpRequest, HttpResponse, Result};
use actix_ws::Message;
use futures_util::StreamExt;
use actix_web::rt::spawn;

#[get("/ws/test_websocket")]
pub async fn test_websocket_middleware(req: HttpRequest, body: actix_web::web::Payload,) -> Result<HttpResponse> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?; // OnConnected
    let welcome_msg = "Example string sent from Server immediately at WebSocket connection acceptance.";
    if let Err(e) = session.text(welcome_msg).await {
        log::error!("Failed to send welcome message: {:?}", e);
    }
    spawn(async move { // keeps connection alive asynchronously
        while let Some(item) = msg_stream.next().await { // OnReceive
            match item {
                Ok(Message::Text(text)) => {
                    if let Err(e) = session.text(format!("{} from Server", text)).await {
                        log::error!("Failed to send text response: {:?}", e);
                        break;
                    }
                }
                Ok(Message::Close(reason)) => {
                    println!("WS closed: {:?}", reason);
                    let _ = session.close(reason).await;
                    break;
                }
                Err(e) => {
                    log::error!("WS error: {:?}", e);
                    break;
                }
                _ => {
                    println!("WS: Unknown message type");
                    break;
                }
            }
        }
    });
    Ok(response)
}