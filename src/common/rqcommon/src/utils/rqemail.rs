use lettre::{message::{header::ContentType, Mailbox, Message},
    transport::smtp::authentication::Credentials,
    SmtpTransport, Transport,
};

pub struct RqEmail {
    sender_email: String,
    sender_password: String,
}

impl RqEmail {
    pub fn init(sender_email: &str, sender_password: &str) -> Self {
        Self {
            sender_email: sender_email.to_string(),
            sender_password: sender_password.to_string(),
        }
    }

    pub fn send(&self, to_address: &str, subject: &str, body: &str, is_body_html: bool,) {
        // Parse sender
        let from: Mailbox = match self.sender_email.parse() {
            Ok(m) => m,
            Err(e) => {
                log::error!("Invalid sender email '{}': {}", self.sender_email, e);
                return;
            }
        };

        // Parse recipient
        let to: Mailbox = match to_address.parse() {
            Ok(m) => m,
            Err(e) => {
                log::error!("Invalid recipient email '{}': {}", to_address, e);
                return;
            }
        };

        // Build message
        let builder = Message::builder()
            .from(from)
            .to(to)
            .subject(subject);

        let email: Message = match if is_body_html {
            builder
                .header(ContentType::TEXT_HTML)
                .body(body.to_string())
        } else {
            builder.body(body.to_string())
        } {
            Ok(e) => e,
            Err(e) => {
                log::error!("Failed to build email message: {}", e);
                return;
            }
        };

        // Credentials
        let creds: Credentials = Credentials::new(self.sender_email.clone(), self.sender_password.clone(),);

        // SMTP Transport
        let mailer: SmtpTransport = match SmtpTransport::relay("smtp.gmail.com") {
            Ok(m) => m.credentials(creds).build(),
            Err(e) => {
                log::error!("Failed to create SMTP transport: {}", e);
                return;
            }
        };

        // Send
        if let Err(e) = mailer.send(&email) {
            log::error!("Failed to send email: {}", e);
            return;
        }

        log::info!("Email successfully sent to {}", to_address);
    }

    pub fn send_text(&self, to: &str, subject: &str, body: &str)
    {
        self.send(to, subject, body, false)
    }

    pub fn send_html(&self, to: &str, subject: &str, body: &str)
    {
        self.send(to, subject, body, true)
    }
}
