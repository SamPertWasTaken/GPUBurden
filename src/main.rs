use crate::configuration::Configuration;

mod configuration;
mod renderer;
mod wayland;

fn main() {
    let config = Configuration::load();
    wayland::start(config);
}
