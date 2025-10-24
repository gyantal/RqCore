// TEMP: implement inside the rqcoresrv project now, but it will go to its own crate later in the /common folder

use ibapi::Client;

pub struct Gateway {
    pub connection_url: String,
    pub client_id: i32,

    // https://github.com/wboayue/rust-ibapi
    // The Client can be shared between threads to support concurrent operations. let client = Arc::clone(&client);
    pub ib_client: Option<Client>,
}

impl Gateway {
    pub fn new(connection_url: &str, client_id: i32) -> Self {
        Self {
            connection_url: connection_url.to_string(),
            client_id,
            ib_client: None,
        }
    }

    pub async fn init(&mut self) {
        println!("Gateway.init() start");
        match Client::connect(&self.connection_url, self.client_id).await {
            Ok(client) => {
                self.ib_client = Some(client);
                log::info!("Connected to TWS at {}", self.connection_url);
            }
            Err(e) => {
                log::error!(
                    "Failed to connect to TWS at {}: {}",
                    self.connection_url, e
                );
            }
        }
    }

    pub async fn exit(&mut self) {
        // Client is automatically disconnected when dropped
        self.ib_client = None;
        log::info!("Disconnected from TWS at {}", self.connection_url);
    }
}

pub struct BrokersWatcher {
    pub gateways: Vec<Gateway>,
}

impl BrokersWatcher {
    #[cfg(target_os = "windows")]
    const GATEWAY_CLIENT_ID: i32 = 201; // gyantal:201, Daya:202, Balazs: 203, TODO: in the future it should come automatically from a function.
    #[cfg(target_os = "linux")]
    const GATEWAY_CLIENT_ID: i32 = 200;

    pub fn new() -> Self {
        Self {
            gateways: Vec::new(),
        }
    }

    pub async fn init(&mut self) {
        println!("BrokersWatcher.init() start");
        // Initialize all gateways with their default configurations
        let connection_url_dcmain = "34.251.1.119:7303"; // port info is fine here. OK. Temporary anyway, and login is impossible, because there are 2 firewalls with source-IP check: AwsVm, IbTWS
        let client_id = Self::GATEWAY_CLIENT_ID;
        let mut gateway0 = Gateway::new(connection_url_dcmain, client_id);
        gateway0.init().await;
        self.gateways.push(gateway0);

        let connection_url_gyantal = "34.251.1.119:7301";
        let mut gateway1 = Gateway::new(connection_url_gyantal, client_id);
        gateway1.init().await;
        self.gateways.push(gateway1);
        
    }

    pub async fn exit(&mut self) {
        for gateway in &mut self.gateways {
            gateway.exit().await;
        }
        self.gateways.clear();
    }
}
