use std::{env, fs, path::Path};

use serde::{Deserialize, Serialize};

pub const SOCKET_FILE: &str = "/tmp/wlt-helper-socket.sock";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
}

impl Config {
    fn get_path() -> String {
        format!(
            "/home/{}/.config/wlt-helper/config.toml",
            env::var("USER").unwrap()
        )
    }

    pub fn get_config() -> Result<Config, String> {
        let path = Config::get_path();
        let config = match fs::read_to_string(Path::new(&path)) {
            Ok(c) => c,
            Err(e) => {
                return Err(format!(
                    "Error occurred while trying to read config at {}: {}",
                    path, e
                ));
            }
        };
        match toml::from_str(&config) {
            Ok(c) => Ok(c),
            Err(e) => Err(format!("Error occurred while parsing config: {}", e)),
        }
    }

    pub fn store(&self) -> Result<(), String> {
        let path = Config::get_path();
        let config = match toml::to_string_pretty(&self) {
            Ok(c) => c,
            Err(e) => {
                return Err(format!("格式化toml文件失败：{}", e));
            }
        };
        match fs::write(path, config) {
            Ok(_) => Ok(()),
            Err(e) => {
                Err(format!("保存配置文件失败：{}", e))
            }
        }
    }
}
