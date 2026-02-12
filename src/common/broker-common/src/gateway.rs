use std::sync::Arc;

use ibapi::{Client, ConnectionOptions};

// ---------- Gateway ----------
pub struct Gateway {
    pub connection_url: String,
    pub client_id: i32,

    // https://github.com/wboayue/rust-ibapi
    // The Client can be shared between threads to support concurrent operations. let client = Arc::clone(&client);

    // The Client is shared safely via Arc so it can be cloned and used across awaits.
    pub ib_client: Option<Arc<Client>>,
}

impl Gateway {
    pub fn new(connection_url: &str, client_id: i32) -> Self {
        Self { connection_url: connection_url.to_string(), client_id, ib_client: None }
    }

    pub async fn init(&mut self) {
        log::debug!("Gateway.init() start");

        // tcp_no_delay: "Order submissions: 0-40ms latency reduction (small writes sent immediately)"
        // "Nagle's algorithm is a TCP optimization technique designed to improve network efficiency by reducing the number of small packets
        // sent over the network. It works by buffering and combining small outgoing data chunks into larger packets before transmission,
        // rather than sending them immediately. Specifically, it delays sending new data if there is unacknowledged data already in flight,
        // waiting until either an acknowledgment (ACK) is received or enough data accumulates to fill a full TCP segment.
        // This helps minimize overhead from packet headers, especially on slower or congested networks, but it can introduce latency"
        // Typical latency you might save (rule of thumb): (~40-200-500ms)
        //      Windows: delayed ACK timer is 200 ms, so a “Nagle + delayed ACK” interaction can show up as ~200 ms stalls in certain write patterns.
        //      Linux/RHEL: people commonly see ~40 ms-ish delays (and it’s tunable; RHEL shows knobs like tcp_delack_min).
        //      TCP specs allow ACKs to be delayed but must happen within 500 ms (upper bound).
        let options = ConnectionOptions::default().tcp_no_delay(true).startup_callback(|msg| {
            // When TWS sends messages like OpenOrder or OrderStatus during the connection handshake, this callback processes them instead of discarding.
            // AccountInfo connection startup messages are handled by ib-api. Only not-recognized (unsolicited) messages arrive here. If there is no OpenOrder, this is not called ever.
            println!("TWS connection established. startup_callback()");
            println!("TWS connection established. startup_callback() msg: {:#?}", msg);
        });

        match Client::connect_with_options(&self.connection_url, self.client_id, options).await {
            Ok(client) => {
                self.ib_client = Some(Arc::new(client));
                log::info!("Connected to TWS at {}", self.connection_url);
            }
            Err(e) => {
                log::error!("Failed to connect to TWS at {}: {}", self.connection_url, e);
            }
        }
    }

    pub async fn exit(&mut self) {
        // Client is automatically disconnected when dropped
        self.ib_client = None; // disconnect on drop
        log::info!("Disconnected from TWS at {}", self.connection_url);
    }
}
