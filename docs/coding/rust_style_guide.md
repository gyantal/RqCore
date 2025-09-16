# RqCore Rust Style Guide



## Folder structure

- For file names (and variable names), use use snake\_case: all lowercase, underscore (not the hyphen, because '-' can mean subtraction, so it is never good for variable names)

- For folder names: also lowercase, and use \_ underscore in general. That is the Rust convention (although mixed). However, if the folder is a top-level folder that contains a crate (it has Cargo.toml in it), then the crate name is hyphen (-). E.g. Cargo name = "actix-router", then the folder name uses hyphen: e.g. "actix-router" (this is because the Cargo package manager treat names with hyphen, not underscore). This is the only exception in Rust convention.

