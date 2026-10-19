//! Developer CLI: run any operation from a JSON request, for manual QA.
//!
//!   cargo run -p radial-core --example op -- '{"source":"/abs/in.jpg","operation":"metadata",
//!     "destDir":"/abs/out","stem":"x","metadata":{"fields":{"title":"Hi"}}}'
//!
//! Prints the outcome or the classified error as JSON.

use radial_core::ops::{self, OperationRequest};
use radial_core::Registry;
use std::sync::atomic::AtomicBool;

fn main() {
    let Some(json) = std::env::args().nth(1) else {
        eprintln!("usage: op '<OperationRequest JSON>'");
        std::process::exit(2);
    };
    let req: OperationRequest = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("invalid request: {e}");
            std::process::exit(2);
        }
    };
    match ops::run(&Registry::new(), &req, &AtomicBool::new(false), |s| {
        eprintln!("  {s:?}")
    }) {
        Ok(o) => println!("{}", serde_json::to_string_pretty(&o).unwrap()),
        Err(e) => {
            println!("{}", serde_json::to_string_pretty(&e).unwrap());
            std::process::exit(1);
        }
    }
}
