use std::sync::{Arc, LazyLock, Mutex};
use std::env;

// TODO: implement inside the rqcoresrv project now, but it will go to its own crate later in the /common folder

use ibapi::Client;

// ---------- Global ----------
pub static RQ_BROKERS_WATCHER: LazyLock<BrokersWatcher> = LazyLock::new(|| BrokersWatcher::new());

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
        Self {
            connection_url: connection_url.to_string(),
            client_id,
            ib_client: None,
        }
    }

    pub async fn init(&mut self) {
        log::debug!("Gateway.init() start");
        match Client::connect(&self.connection_url, self.client_id).await {
            Ok(client) => {
                self.ib_client = Some(Arc::new(client));
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
        self.ib_client = None; // disconnect on drop
        log::info!("Disconnected from TWS at {}", self.connection_url);
    }
}

pub struct BrokersWatcher {
    // TODO: use Arc<Mutex<BrokersWatcher>>
    // This Mutex will assures that only 1 thread can access the BrokerWatcher, which is too much restriction,
    // because 1. it can be multithreaded, or that if it contains 2 clients, those 2 clients should be accessed parallel.
    // However, it will suffice for a while. Yes. We will need the mutex at lower level later.

    // gateways are Arc<Mutex<Gateway>> so each gateway can be locked independently
    pub gateways: Mutex<Vec<Arc<Mutex<Gateway>>>>,
}

impl BrokersWatcher {
    pub fn new() -> Self {
        BrokersWatcher { gateways: Mutex::new(Vec::new()) }
    }

    pub fn gateway_client_id() -> i32 {
    if env::consts::OS == "windows" { // On windows, use USERDOMAIN, instead of USERNAME, because USERNAME can be the same on multiple machines (e.g. "gyantal" on both GYANTAL-PC and GYANTAL-LAPTOP)
        let userdomain = env::var("USERDOMAIN").expect("Failed to get USERDOMAIN environment variable");
        match userdomain.as_str() {
            "GYANTAL-PC" => 210,
            "GYANTAL-LAPTOP" => 211,
            "BALAZS-PC" => 212,
            "BALAZS-LAPTOP" => 213,
            "DAYA-DESKTOP" => 214,
            "DAYA-LAPTOP" => 215,
            "DRCHARMAT-LAPTOP" => 216,
            _ => panic!("Windows user name is not recognized. Add your username and folder here!"),
        }
    } else { // Linux and MacOS
        200
    }
}

    pub async fn init(&self) {
        log::info!("BrokersWatcher.init() start");
        // Initialize all gateways with their default configurations
        let connection_url_dcmain = "34.251.1.119:7303"; // port info is fine here. OK. Temporary anyway, and login is impossible, because there are 2 firewalls with source-IP check: AwsVm, IbTWS
        let client_id = Self::gateway_client_id();
        let mut gateway0 = Gateway::new(connection_url_dcmain, client_id);
        gateway0.init().await;
        let mut gateways = self.gateways.lock().unwrap();
        gateways.push(Arc::new(Mutex::new(gateway0)));

        let connection_url_gyantal = "34.251.1.119:7301";
        let mut gateway1 = Gateway::new(connection_url_gyantal, client_id);
        gateway1.init().await;
        gateways.push(Arc::new(Mutex::new(gateway1)));
    }

    pub async fn exit(&self) {
        let mut gateways = self.gateways.lock().unwrap();
        for gateway in gateways.iter() {
            gateway.lock().unwrap().exit().await;
        }
        gateways.clear();
    }
}
