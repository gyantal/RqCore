# RqCore Rust Style Guide

## Naming Conventions

- **Types**: PascalCase (e.g., `MessageBus`, `AccountSummary`)
- **Functions/Methods**: snake_case (e.g., `connect`, `server_time`)
- **Constants**: UPPER_SNAKE_CASE (e.g., `MAX_RECONNECT_ATTEMPTS`)
- **Module names**: snake_case (e.g., `market_data`, `order_management`)

## Comments

- **Keep comments concise and avoid redundancy**. Don't state the obvious.
  - ✅ Good: Complex logic that needs explanation
  - ❌ Bad: `Connection::new(); // Initialize connection`
  - ❌ Bad: `flag = true ;// Set flag to true`

## Folder structure

- For file names (and variable names), use use **snake_case**: all lowercase, underscore (not the hyphen, because '-' can mean subtraction, so it is never good for variable names)

- For folder names: also lowercase, and use _ underscore in general. That is the Rust convention (although mixed). However, if the folder is a top-level folder that contains a crate (it has Cargo.toml in it), then the crate name is hyphen (-). E.g. Cargo name = "actix-router", then the folder name uses hyphen: e.g. "actix-router" (this is because the Cargo package manager treat names with hyphen, not underscore). This is the only exception in Rust convention.


## General code style

+ **Curly brackets {} are mandatory even in single statements** in Rust. It is Rust design, people voted for it. 
But code looks longer (3 lines instead of 2), but if there is a lot of them in a for() loop, then it is annoying to read.
It looks weird, but we can do this, if it is annoying:
if !self.is_trading_allowed { 
    continue; 
}
=> 
if !self.is_trading_allowed
    { continue; }



