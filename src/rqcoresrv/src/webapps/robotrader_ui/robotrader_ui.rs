use actix_ws::{Message};
use actix_web::{get, HttpRequest, HttpResponse, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use actix_identity::Identity;

#[get("/ws/robotrader_websocket")]
pub async fn robotrader_websocket(req: HttpRequest, body: actix_web::web::Payload, identity: Option<Identity>,) -> Result<HttpResponse> {
    let (response, mut ws_session, mut msg_stream) = actix_ws::handle(&req, body)?;
    log::info!("WebSocket session Opened");
    let user_email = identity.and_then(|id| id.id().ok()).unwrap_or("anonymous".to_string());
    log::info!("robotrader_websocket - user = {}", user_email);

    let handshake_msg = json!({"type": "onconnected", "user": user_email});
    if let Err(e) = ws_session.text(handshake_msg.to_string()).await {
        log::error!("WebSocket send error: {}", e);
    }

    // Spawn websocket message handler
    actix_web::rt::spawn(async move {
        while let Some(client_msg) = msg_stream.next().await {
            match client_msg {
                Ok(Message::Text(text)) => {
                    log::info!("robotrader_websocket message: {}", text);
                    let client_request: Value;
                    if let Ok(parsed_request) = serde_json::from_str(&text) {
                        client_request = parsed_request;
                    } else {
                        let server_response = json!({
                            "type": "error",
                            "message": "Invalid JSON format"
                        });
                        let _ = ws_session.text(server_response.to_string()).await;
                        continue;
                    }
                    // Extract request type
                    let request_type = client_request["type"].as_str().unwrap_or("");
                    match request_type {
                        "getexecutedorders" => { // Example request
                            let executed_orders = vec![json!({"symbol": "AAPL", "price": 190}),json!({"symbol": "TSLA", "price": 220})]; // Example response data
                            let server_response = json!({
                                "type": "executed_orders",
                                "data": executed_orders
                            });

                            if ws_session.text(server_response.to_string()).await.is_err() {
                                break;
                            }
                        }
                        _ => { // Unknown command
                            let server_response = json!({
                                "type": "error",
                                "message": format!("Unknown command: {}", request_type)
                            });
                            let _ = ws_session.text(server_response.to_string()).await;
                        }
                    }
                }
                Ok(Message::Close(reason)) => {
                    println!("WebSocket closed: {:?}", reason);
                    let _ = ws_session.close(reason).await;
                    break;
                }
                Err(e) => {
                    log::error!("WebSocket error: {:?}", e);
                    break;
                }
                _ => {
                    log::info!("WS: Unsupported message type");
                    continue;
                }
            }
        }
        log::info!("WebSocket session ended");
    });
    Ok(response)
}
