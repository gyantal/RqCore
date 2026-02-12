use rqcommon::log_and_println;
use broker_common::brokers_watcher::RqOrder;

use crate::RQ_BROKERS_WATCHER;

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

}

impl RoboTrader {
    pub async fn place_orders(strategy_name: &str, orders: Vec<RqOrder>, is_simulation: bool) {
        if orders.is_empty() {
            log_and_println!("RoboTrader.place_orders({}): no orders.", strategy_name);
            return;
        }

        RQ_BROKERS_WATCHER.place_orders(orders, is_simulation).await;
    }
}
