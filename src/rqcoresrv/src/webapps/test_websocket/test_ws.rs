use actix_web::{get, HttpRequest, HttpResponse, Result};
use actix_ws::Message;
use futures_util::StreamExt;
use actix_web::rt::spawn;

#[get("/ws/test_websocket")]
pub async fn test_websocket_middleware(req: HttpRequest, body: actix_web::web::Payload,) -> Result<HttpResponse> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?; // OnConnected
    let welcome_msg = "Example string sent from Server immediately at WebSocket connection acceptance.";
    let _ = session.text(welcome_msg).await;
    spawn(async move { // keeps connection alive asynchronously
        while let Some(item) = msg_stream.next().await { // OnReceive
            match item {
            Ok(Message::Text(text)) => {
                let _ = session.text(format!("{} from Server", text)).await;
            }
            Ok(Message::Close(reason)) => {
                println!("WS closed: {:?}", reason);
                let _ = session.close(reason).await;
                break;
            }
            Err(e) => {
                println!("WS error: {:?}", e);
                break;
            }
            _ => {}
            }
        }
    });
    Ok(response)
}