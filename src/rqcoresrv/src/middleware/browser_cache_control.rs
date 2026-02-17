use actix_web::{Error, HttpResponse, Responder, body::MessageBody, dev::ServiceResponse, get, http::header::{self, CACHE_CONTROL, HeaderValue}, middleware::Next};

// Middleware function to add 30-day cache headers
pub async fn browser_cache_control_30_days_middleware<B>(
    req: actix_web::dev::ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<impl MessageBody>, Error>
where
    B: MessageBody + 'static,
{
    // To test whether index.html is coming from browser cache or reloaded: in Chrome, F12 (Disable cache is OFF), and type the URL into the URL bar. Don't press the Reload icon.
    // Pressing Reload icon always fetches it from the server (even if it should be cached.) Type, retype the https://rqcore.com/ in the URL bar.

    // If needed in the future for website version updates, 'all domain' server-side browser-cache-busting can be done with Response header Clear-Site-Data: "cache" (HTTPS only)

    // let path = req.path().to_string();
    let mut res = next.call(req).await?;

    // if path == "/" || matches!(path.as_str(), "/useraccount/login" | "/useraccount/logout")
    // {
    //     res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"));
    // } else {
    //     res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("public, max-age=2592000"));
    // }
    res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("public, max-age=2592000")); // 2592000 = 30 days in seconds
    Ok(res)
}

// Middleware endpoint to inform the browser to delete local cache for the whole domain. (if we ever need forced cache clearing initiated from JS)
// This can be called by JS clients.
// A webapp locally stored, cached JS can check its version number against a version number on the server coming in a realtime, always current websocket message.
// If it notices difference it can ask server by calling this endpoint to delete local cache for the domain.
#[get("/browser_domain_cache_bust_header")]
pub async fn browser_domain_cache_bust_header() -> impl Responder {
    HttpResponse::Ok()
    .insert_header((header::CONTENT_TYPE, "text/html; charset=utf-8"))
    .append_header((header::CLEAR_SITE_DATA, "\"cache\""))
    .body(format!("<h2>Hello.</h2>The clear-site-data: \"cache\" header will force disk cache to be cleared for the domain.<br>Test it on live, because localhost works differently."))
}
