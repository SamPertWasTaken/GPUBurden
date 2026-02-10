use std::{env, path::{Path, PathBuf}};

use config::{Config, File};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct MonitorConfig {
    pub name: String,
    pub shader: String
}

#[derive(Clone, Debug, Deserialize)]
pub struct Configuration {
    monitors: Vec<MonitorConfig>
}
impl Configuration {
    pub fn load() -> Option<Configuration> {
        let config_path = Configuration::locate_config_path();
        if config_path.is_none() {
            println!("Config path not found.");
            println!("Backing up to default.");
            return None;
        }
        let config_path = config_path.unwrap();

        let config_file = Configuration::locate_config(&config_path);
        if config_file.is_none() {
            println!("Config file not found.");
            println!("Backing up to default.");
            return None;
        }

        let config_build = Config::builder()
            .add_source(File::from(config_file.unwrap()))
            .build();
        if let Err(err) = config_build {
            println!("Failed to build configuration from file: {err}");
            println!("Backing up to default.");
            return None;
        }

        let config = config_build.unwrap().try_deserialize::<Configuration>();
        if let Err(err) = config {
            println!("Failed to deserialize configuration from file: {err}");
            println!("Backing up to default.");
            return None;
        }
        let mut config = config.unwrap();

        // convert all shaders into their paths 
        let config_path_string = config_path.to_str();
        for monitor in &mut config.monitors {
            monitor.shader = format!("{}/{}", config_path_string.unwrap(), monitor.shader);
        }

        Some(config)
    }
    pub fn monitor_config(&self, name: &str) -> Option<MonitorConfig> {
        for monitor in &self.monitors {
            if monitor.name != name {
                continue;
            }
            return Some(monitor.clone());
        }

        None
    }

    fn locate_config_path() -> Option<PathBuf> {
        if let Ok(mut config_home) = env::var("XDG_CONFIG_HOME") {
            config_home.push_str("/gpuburden");
            let path = PathBuf::from(config_home);
            if path.exists() {
                return Some(path);
            } 

            return None;
        }
        if let Ok(mut user_home) = env::var("HOME") {
            user_home.push_str("/.config/gpuburden");
            let path = PathBuf::from(user_home);
            if path.exists() {
                return Some(path);
            }

            return None;
        }

        None
    }
    fn locate_config(config_path: &Path) -> Option<PathBuf> {
        let mut path = config_path.to_path_buf();
        path.push(Path::new("gpuburden.toml"));
        if path.exists() {
            return Some(path);
        } 

        None
    }
}
