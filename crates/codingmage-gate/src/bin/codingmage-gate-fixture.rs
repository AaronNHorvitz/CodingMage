//! Deterministic process fixture for gate integration tests.

use std::{thread, time::Duration};

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("pass") => print!("pass"),
        Some("fail") => std::process::exit(7),
        Some("sleep") => thread::sleep(Duration::from_secs(5)),
        Some("noisy") => print!("{}", "x".repeat(4096)),
        _ => std::process::exit(9),
    }
}
