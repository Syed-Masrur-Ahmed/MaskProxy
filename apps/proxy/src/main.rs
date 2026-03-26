// main.rs
// Entry point for the MaskProxy Rust proxy server.
// Responsibilities:
//   - Load config (port, upstream URL, Redis URL, model path)
//   - Build the MaskProxy struct with all dependencies (Redis, NER, Router)
//   - Register MaskProxy as the Pingora ProxyHttp handler
//   - Start the Pingora server (which spins up the Tokio runtime + thread pool)

mod proxy;
mod masker;
mod rehydrator;
mod router;
mod state;

fn main() {
    // TODO: initialise tracing/logging (OpenTelemetry)
    // TODO: load config from env vars or config file
    // TODO: build Redis connection pool  (state::redis)
    // TODO: build NER model session      (masker::ner)
    // TODO: build LanceDB client         (router)
    // TODO: construct MaskProxy and hand it to Pingora
    // TODO: call server.run_forever()

    println!("MaskProxy proxy starting on :8080");
}
