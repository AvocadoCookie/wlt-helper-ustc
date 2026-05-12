use std::{env, fs, path::Path};

use serde::{Deserialize, Serialize};

pub mod nm;

pub const SOCKET_FILE: &str = "/tmp/wlt-helper-socket.sock";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("读取配置文件失败：{0}")]
    ConfigRead(#[source] std::io::Error),
    #[error("解析配置文件失败：{0}")]
    ConfigParse(#[source] toml::de::Error),
    #[error("保存配置文件失败：{0}")]
    ConfigWrite(#[source] std::io::Error),
    #[error("序列化配置失败：{0}")]
    ConfigSerialize(#[source] toml::ser::Error),
    #[error("密钥环操作失败：{0}")]
    Keyring(String),
    #[error("DBus 操作失败：{0}")]
    Dbus(#[source] zbus::Error),
    #[error("网络请求失败：{0}")]
    Http(#[source] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::ConfigParse(e)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Error::ConfigSerialize(e)
    }
}

impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        Error::Dbus(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
}

impl Config {
    fn get_path() -> String {
        format!(
            "/home/{}/.config/wlt-helper/config.toml",
            env::var("USER").unwrap_or_default()
        )
    }

    pub fn get_config() -> Result<Config, Error> {
        let path = Config::get_path();
        let config = fs::read_to_string(Path::new(&path)).map_err(Error::ConfigRead)?;
        let config = toml::from_str(&config)?;
        Ok(config)
    }

    pub fn store(&self) -> Result<(), Error> {
        let path = Config::get_path();
        let config = toml::to_string_pretty(&self)?;
        fs::write(&path, config).map_err(Error::ConfigWrite)?;
        Ok(())
    }
}
