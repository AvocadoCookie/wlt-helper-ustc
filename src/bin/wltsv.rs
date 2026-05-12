use std::{collections::HashMap, fs};

use oo7::{Keyring, Secret};
use reqwest::Client;
use tokio::{io::AsyncReadExt, net::UnixListener, sync::OnceCell};
use tracing::Level;
use wlt_helper::{
    Config, Error, SOCKET_FILE,
    nm::{
        Code, ConnectivityState, NetworkManagerProxy, connection::active::ActiveConnectionProxy,
        device::DeviceProxy, ip4config::IP4ConfigProxy,
    },
};
use zbus::Connection;

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:150.0) Gecko/20100101 Firefox/150.0";

static KEYRING: OnceCell<Keyring> = OnceCell::const_new();
static CLIENT: OnceCell<Client> = OnceCell::const_new();

async fn get_name_password() -> Result<(String, String), Error> {
    let config = Config::get_config()?;

    let keyring = KEYRING.get().unwrap();
    let mut attr = HashMap::new();
    attr.insert("app", "wlt-helper");
    attr.insert("user", &config.name);

    let items = keyring
        .search_items(&attr)
        .await
        .map_err(|e| Error::Keyring(e.to_string()))?;
    if items.is_empty() {
        return Err(Error::Other("未找到已存储的密码".into()));
    }

    let password = match items.first().unwrap().secret().await {
        Ok(s) => match &s {
            Secret::Text(t) => t.to_owned(),
            Secret::Blob(_) => {
                return Err(Error::Other("密码必须以文本格式存储".into()));
            }
        },
        Err(e) => {
            return Err(Error::Keyring(e.to_string()));
        }
    };

    Ok((config.name, password))
}

async fn need_log_in() -> Result<bool, Error> {
    let conn = Connection::system().await?;
    let proxy = NetworkManagerProxy::new(&conn).await?;

    let primary = proxy.primary_connection().await?;
    tracing::debug!("主连接：{}", primary);

    let ac_proxy = ActiveConnectionProxy::builder(&conn)
        .path(primary.as_str())
        .unwrap()
        .build()
        .await?;

    let ac_type = ac_proxy.r#type().await?;
    tracing::debug!("{} -> {}", primary, ac_type);

    let devices = ac_proxy.devices().await?;
    if devices.is_empty() {
        return Err(Error::Other(format!("连接 {} 无设备", primary)));
    }

    let de_proxy = DeviceProxy::builder(&conn)
        .path(devices[0].to_owned())
        .unwrap()
        .build()
        .await?;

    // 检查是否有网，有网则跳过登录
    let _ = proxy.check_connectivity().await?;
    let connectivity = de_proxy.ip4_connectivity().await?;
    if connectivity == ConnectivityState::Full.code() {
        tracing::debug!("网络已连通");
        return Ok(false);
    }

    // 如果是有线连接，则检查IP地址
    if ac_type == "802-3-ethernet" {
        let ic_path = de_proxy.ip4_config().await?;
        let ic_proxy = IP4ConfigProxy::builder(&conn)
            .path(ic_path)
            .unwrap()
            .build()
            .await?;

        let address = ic_proxy.address_data().await?;
        if address.is_empty() {
            return Err(Error::Other("无可用 IP 信息".into()));
        }
        let address = address[0].to_owned();
        let ip_val = address
            .get("address")
            .ok_or_else(|| Error::Other("IP 地址数据缺失".into()))?;
        let ip = String::try_from(ip_val.clone())
            .map_err(|e| Error::Other(format!("IP 地址类型转换失败：{:?}", e)))?;

        if ip.starts_with("114.214.") {
            return Ok(true);
        } else {
            return Ok(false);
        }
    }

    // 如果是无线连接，则检查ID
    if ac_type == "802-11-wireless" {
        let ac_id = ac_proxy.id().await?;
        tracing::debug!("{} -> {}", primary, ac_id);

        if ac_id == "ustcnet" {
            return Ok(true);
        } else {
            return Ok(false);
        }
    }

    Err(Error::Other("未知的连接类型".into()))
}

async fn log_in() -> Result<(), Error> {
    // 检查是否需要登录
    match need_log_in().await {
        Ok(flag) => {
            if !flag {
                tracing::info!("未检测到网络通连接，或网络通已登录");
                return Ok(());
            }
        }
        Err(e) => {
            return Err(e);
        }
    };
    tracing::info!("检测到未登录的网络通连接，尝试登录");

    let client = match CLIENT
        .get_or_init(|| async {
            Client::builder()
                .user_agent(UA)
                .build()
                .expect("创建 HTTP 客户端失败")
        })
        .await
    {
        _c => _c,
    };

    let (name, password) = get_name_password().await?;

    let form_param = [
        ("cmd", "set"),
        ("go", " 开通网络 "),
        ("name", &name),
        ("password", &password),
        ("type", "0"),
        ("exp", "0"),
    ];
    let response = client
        .get("http://wlt.ustc.edu.cn/cgi-bin/ip/")
        .form(&form_param)
        .send()
        .await?;

    if response.headers().contains_key("set-cookie") {
        tracing::info!("登录成功");
        Ok(())
    } else {
        Err(Error::Other(format!("登录失败，响应：{:?}", response)))
    }
}

#[tokio::main]
async fn main() {
    let log_level = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    KEYRING
        .get_or_init(|| async { Keyring::new().await.unwrap() })
        .await;

    let _ = fs::remove_file(SOCKET_FILE);

    let listener = UnixListener::bind(SOCKET_FILE).unwrap();

    if let Err(e) = log_in().await {
        tracing::error!("{}", e);
    }

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let mut buf = [0; 1024];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => {
                        break;
                    }
                    Ok(n) => {
                        tracing::debug!("received: {}", String::from_utf8_lossy(&buf[..n]));

                        if &buf[..n] == b"LOGIN" || &buf[..n] == b"REFRESH" {
                            if let Err(e) = log_in().await {
                                tracing::error!("{}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("读取 socket 失败：{}", e);
                        break;
                    }
                }
            }
        });
    }
}
