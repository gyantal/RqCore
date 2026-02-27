use std::sync::OnceLock;
use lettre::{message::{header::ContentType, Mailbox, Message},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};

pub struct RqEmail {
    pub sender_email: String,
    pub sender_password: String,
}

// ---------- Global static variables ----------
pub static RQEMAIL: OnceLock<RqEmail> = OnceLock::new();    // Lock contains the global RqEmail instance; OnceLock allows us to initialize it once at runtime

impl RqEmail {
    pub fn init(sender_email: &str, sender_password: &str) {
        RQEMAIL.set(RqEmail {
            sender_email: sender_email.to_string(),
            sender_password: sender_password.to_string(),
        }).ok();
    }

    pub async fn send(to_address: &str, subject: &str, body: &str, is_body_html: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let g_rqemail = match RQEMAIL.get() { // g_rqemail is the global RqEmail instance
            Some(e) => e,
            None => {
                log::error!("RqEmail not initialized");
                return Err(std::io::Error::other("RqEmail not initialized").into());
            }
        };

        let from_mailbox: Mailbox = match g_rqemail.sender_email.parse() {
            Ok(m) => m,
            Err(e) => {
                log::error!("Invalid sender email '{}': {}", g_rqemail.sender_email, e);
                return Err(e.into());
            }
        };

        // Parse To address as recipient
        let to_mailbox: Mailbox = match to_address.parse() {
            Ok(m) => m,
            Err(e) => {
                log::error!("Invalid recipient email '{}': {}", to_address, e);
                return Err(e.into());
            }
        };

        // Build message
        let builder = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject);

        let message_result = if is_body_html {
            builder
                .header(ContentType::TEXT_HTML)
                .body(body.to_string())
        } else {
            builder.body(body.to_string())
        };

        let message: Message = match message_result {
            Ok(m) => m,
            Err(e) => {
                log::error!("Failed to build email message: {}", e);
                return Err(e.into());
            }
        };

        // Credentials
        let creds: Credentials = Credentials::new(g_rqemail.sender_email.clone(), g_rqemail.sender_password.clone(),);

        // SMTP Transport
        let mailer: AsyncSmtpTransport<Tokio1Executor> = match AsyncSmtpTransport::<Tokio1Executor>::relay("smtp.gmail.com") {
            Ok(m) => m.credentials(creds).build(),
            Err(e) => {
                log::error!("Failed to create SMTP transport: {}", e);
                return Err(e.into());
            }
        };

        // Send
        if let Err(e) = mailer.send(message).await {
            log::error!("Failed to send email: {}", e);
            return Err(e.into());
        }

        log::info!("Email successfully sent to {}", to_address);
        Ok(())
    }

    pub async fn send_text(to: &str, subject: &str, body: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    {
        RqEmail::send(to, subject, body, false).await
    }

    pub async fn send_html(to: &str, subject: &str, body: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    {
        RqEmail::send(to, subject, body, true).await
    }
}
