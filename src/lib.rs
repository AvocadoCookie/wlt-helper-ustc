use std::{borrow::Cow, env, fs, path::Path};
use url::Url;
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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckMethod {
    Nm,
    Ping,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PingConfig {
    #[serde(default = "default_interval")]
    pub interval: u32,
    #[serde(default = "default_site")]
    pub site: Cow<'static, str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    #[serde(rename = "type", default = "default_type")]
    pub r#type: u32,
    #[serde(default = "default_exp")]
    pub exp: u32,
    #[serde(default = "default_check")]
    pub check: CheckMethod,
    #[serde(default)]
    pub ping: PingConfig,
}

const fn default_type() -> u32 {
    0
}

const fn default_exp() -> u32 {
    3600
}

const fn default_check() -> CheckMethod {
    CheckMethod::Ping
}

const fn default_interval() -> u32 {
    3600
}

const fn default_site() -> Cow<'static, str> {
    Cow::Borrowed("https://www.baidu.com/")
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
        let config_str = fs::read_to_string(Path::new(&path)).map_err(Error::ConfigRead)?;
        let mut config: Config = toml::from_str(&config_str)?;

        const VALID_EXP: &[u32] = &[0, 3600, 14400, 39600, 50400];
        let mut corrected = false;

        if config.r#type > 8 {
            tracing::warn!(
                "配置项 type 的值 {} 不合法（应为 0~8），已重置为默认值 0",
                config.r#type
            );
            config.r#type = default_type();
            corrected = true;
        }

        if !VALID_EXP.contains(&config.exp) {
            tracing::warn!(
                "配置项 exp 的值 {} 不合法（应为 0/3600/14400/39600/50400），已重置为默认值 3600",
                config.exp
            );
            config.exp = default_exp();
            corrected = true;
        }

        if Url::parse(&config.ping.site).is_err() {
            tracing::warn!(
                "配置项 site 的值 {} 不是合法的 URI，已重置为默认值",
                config.ping.site
            );
            config.ping.site = default_site();
            corrected = true;
        }

        if corrected {
            config.store()?;
        }

        tracing::debug!("已读取到配置：{:?}", config);

        Ok(config)
    }

    pub fn store(&self) -> Result<(), Error> {
        let path = Config::get_path();
        let config = toml::to_string_pretty(&self)?;
        fs::write(&path, config).map_err(Error::ConfigWrite)?;
        Ok(())
    }
}
