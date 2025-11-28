# rqcoresrv

Rust Actix-web HTTPS server serving a TypeScript-built frontend.

## Prerequisites

- Node.js + npm

## Frontend build.
Use NodeJS to convert and minify TS to JS

```bash
npm install
npm run build
```

"npm run build" compiles and minifies TypeScript and bundles to `static/test_ts2js/test_ts2js.js`.
Test it as: 
[test_ts2js](https://127.0.0.1:8443/test_ts2js)
[test_ts2js/](https://127.0.0.1:8443/test_ts2js/)
[test_ts2js/index.html](https://127.0.0.1:8443/test_ts2js/index.html)


## Run server

```bash
cargo build
cargo run
```

