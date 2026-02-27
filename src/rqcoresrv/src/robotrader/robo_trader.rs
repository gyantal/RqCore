use std::{collections::HashMap, sync::{LazyLock, Mutex}};

use ibapi::orders::{CommissionReport, ExecutionData};
use rqcommon::log_and_println;
use rqcommon::rqhelper::MutexExt;
use broker_common::brokers_watcher::{BrokerClient, RqOrder};

use crate::RQ_BROKERS_WATCHER;

// ---------- Global static variables ----------
pub static RQ_ROBO_TRADER: LazyLock<RoboTrader> = LazyLock::new(|| RoboTrader::new());

// ---------- RoboTrader ----------
// RobotTrader main functions:
// 1. For a given portfolio strategy, call the appropriate handler class to determine what tickers to trade and what $Value to trade. (based on portfolio PV)
// 2. Place orders to the broker (e.g. IB) via BrokersWatcher. This can be a fire and forget method if MOC orders are used.
// But it should register these virtual orders internally to know which virtual orders belong to which strategy.
// RQ_BROKERS_WATCHER.place_orders() should return the order ids, and order details (numShares)
// MOC execution happens just at the end of the day.
// 3. Monitor the orders until they are filled, and log the fills (e.g. fill price, fill time, etc.). Callbacks or periodic polling.
// If polling senses that all intraday virtual orders are filled, then it sends a TradeReport email. Insert trades to SQL and flags those virtual trades as completed.
// 4. 30 min after market closes, reread all the broker orders, and try to figure out which orders should go to which strategy_name.
// Find matching strategy_name for orders. Split orders if needed. It was possible that 3 strategies gave different Buy/Sell orders for AAPL.
// Broker executes the aggregate order, but that real order should be split to 3 virtual orders.
// Register them in SQL. Send an daily TradeReport email to the user with the order details (e.g. ticker, numShares, fill price, fill time, etc.).
pub struct RoboTrader {
    pub order_executions: Mutex<HashMap<BrokerClient, (Vec<ExecutionData>, Vec<CommissionReport>)>>,
}

impl RoboTrader {
    fn new() -> Self {
        Self { order_executions: Mutex::new(HashMap::new()) }
    }

    pub async fn init(&self) {
        self.refresh_executions().await;
    }

    pub async fn exit(&self) {
    }

    pub async fn refresh_executions(&self) {
        let mut order_executions = self.order_executions.lock_ignore_poison();
        order_executions.clear();

        let executions_dcmain = RQ_BROKERS_WATCHER.get_order_executions(BrokerClient::DcMain).await;
        for execution_data in executions_dcmain.0.iter() {
            // log_and_println!("RoboTrader.init(): DcMain execution: {:#?}", execution_data);
            log_and_println!("RoboTrader.init(): DcMain execution: {} {} {} {}", execution_data.contract.symbol, execution_data.execution.shares, execution_data.execution.price, execution_data.execution.time);
        }
        order_executions.insert(BrokerClient::DcMain, executions_dcmain);

        let executions_dcblanzac = RQ_BROKERS_WATCHER.get_order_executions(BrokerClient::DcBlanzac).await;
        order_executions.insert(BrokerClient::DcBlanzac, executions_dcblanzac);

        let executions_gyantal = RQ_BROKERS_WATCHER.get_order_executions(BrokerClient::Gyantal).await;
        order_executions.insert(BrokerClient::Gyantal, executions_gyantal);
    }

    pub async fn place_orders(strategy_name: &str, orders: Vec<RqOrder>, is_simulation: bool, user_log: &mut String) {
        if orders.is_empty() {
            log_and_println!("RoboTrader.place_orders({}): no orders.", strategy_name);
            return;
        }

        RQ_BROKERS_WATCHER.place_orders(orders, is_simulation, user_log).await;
    }
}
