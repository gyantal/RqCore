pub struct ServerIp;

impl ServerIp {
    pub fn localhost_loopback_with_ip() -> &'static str { // "127.0.0.1" is the 'same' but better than "localhost": it avoids costly DNS name resolution
        "127.0.0.1"  // loopback address (works even if machine has no network card); in Debug when you test the web service and want to reach it from the local machine
    }

    pub fn localhost_meta_all_private_ip_with_ip() -> &'static str {  // 0.0.0.0 means all IPv4 addresses on the local machine (equals LocalHost_127.0.0.1 Plus all private IPs of the computer), cannot be used for target, only for listening
        "0.0.0.0"  // use 0.0.0.0 in Production when you want that it is accessible from the Internet (binding to all local private IPs)
    }

    pub fn sq_core_server_public_ip_for_clients() -> &'static str {
        "34.251.1.119"
    }

    pub fn health_monitor_public_ip() -> &'static str {
        if std::env::consts::OS == "windows" {
            // return Self::localhost_loopback_with_ip();       // sometimes for clients running on Windows (in development), we want localHost if Testing new HealthMonitor features
            "23.20.243.199"
        } else {
            "23.20.243.199"
        }
    }

    pub fn health_monitor_public_ipv6() -> String {
        format!("::ffff:{}", Self::health_monitor_public_ip())
    }

    pub const DEFAULT_HEALTH_MONITOR_SERVER_PORT: u16 = 52100;

    // port info is fine here. As login is impossible from other machines, because there are 2 firewalls with source-IP check: AwsVm, IbTWS
    pub const IB_SERVER_PORT_GYANTAL: u16 = 7301;
    pub const IB_SERVER_PORT_DCMAIN: u16 = 7303;
}
