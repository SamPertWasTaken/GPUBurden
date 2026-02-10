use std::{panic, time::Instant};

use crate::configuration::Configuration;

mod configuration;
mod renderer;
mod wayland;

const ERROR_TIMEOUT_SECS: u64 = 30;

fn main() {
    let config = Configuration::load();

    let mut last_error: Instant = Instant::now();
    loop {
        let result = panic::catch_unwind(|| {
            wayland::start(config.clone());
        });

        if result.is_ok() {
            break;
        }
        // some error occured
        if last_error.elapsed().as_secs() < ERROR_TIMEOUT_SECS {
            panic!("Two errors occured with {ERROR_TIMEOUT_SECS}s of each other, assuming something's wrong.");
        }

        println!("Caught panic from wayland thread... this is likely a GPU timeout...");
        println!("Restarting...");
        last_error = Instant::now();
    }
}
