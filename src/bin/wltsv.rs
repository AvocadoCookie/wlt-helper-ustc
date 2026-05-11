use std::{collections::HashMap, fs};

use oo7::{Keyring, Secret};
use reqwest::Client;
use tokio::{io::AsyncReadExt, net::UnixListener, sync::OnceCell};
use tracing::Level;
use wlt_helper::{
    Config, SOCKET_FILE,
    nm::{
        Code, ConnectivityState, NetworkManagerProxy, connection::active::ActiveConnectionProxy,
        device::DeviceProxy, ip4config::IP4ConfigProxy,
    },
};
use zbus::Connection;

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:150.0) Gecko/20100101 Firefox/150.0";

static KEYRING: OnceCell<Keyring> = OnceCell::const_new();
static CLIENT: OnceCell<reqwest::Result<Client>> = OnceCell::const_new();

async fn get_name_password() -> Result<(String, String), String> {
    let config = match Config::get_config() {
        Ok(c) => c,
        Err(e) => {
            return Err(e);
        }
    };

    let keyring = KEYRING.get().unwrap();
    let mut attr = HashMap::new();
    attr.insert("app", "wlt-helper");
    attr.insert("user", &config.name);

    let items = match keyring.search_items(&attr).await {
        Ok(items) => items,
        Err(e) => {
            return Err(format!("Error occurred while searching password: {}", e));
        }
    };
    if items.is_empty() {
        return Err(String::from("No password stored"));
    }

    let password = match items.first().unwrap().secret().await {
        Ok(s) => match &s {
            Secret::Text(t) => t.to_owned(),
            Secret::Blob(_) => {
                return Err(String::from("Password must be stored as text"));
            }
        },
        Err(e) => {
            return Err(format!("Error occurred while getting password: {}", e));
        }
    };

    Ok((config.name, password))
}

async fn need_log_in() -> Result<bool, String> {
    let conn = match Connection::system().await {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("DBus连接创建失败：{}", e));
        }
    };
    let proxy = match NetworkManagerProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            return Err(format!("NetworkManager代理创建失败：{}", e));
        }
    };

    let primary = match proxy.primary_connection().await {
        Ok(p) => p,
        Err(e) => {
            return Err(format!("主连接信息获取失败：{}", e));
        }
    };
    tracing::debug!("主连接：{}", primary);

    let ac_proxy = match ActiveConnectionProxy::builder(&conn)
        .path(primary.as_str())
        .unwrap()
        .build()
        .await
    {
        Ok(p) => p,
        Err(e) => {
            return Err(format!("连接信息代理创建失败：{}", e));
        }
    };

    let ac_type = match ac_proxy.r#type().await {
        Ok(t) => t,
        Err(e) => {
            return Err(format!("获取设备类型失败：{}", e));
        }
    };
    tracing::debug!("{} -> {}", primary, ac_type);

    let devices = match ac_proxy.devices().await {
        Ok(d) => d,
        Err(e) => {
            return Err(format!("获取连接设备失败：{}", e));
        }
    };
    if devices.is_empty() {
        return Err(format!("连接{}无设备", primary));
    }

    let de_proxy = match DeviceProxy::builder(&conn)
        .path(devices[0].to_owned())
        .unwrap()
        .build()
        .await
    {
        Ok(p) => p,
        Err(e) => {
            return Err(format!("设备{}代理创建失败：{}", devices[0].to_owned(), e));
        }
    };

    // 检查是否有网，有网则跳过登录
    match proxy.check_connectivity().await {
        Ok(_) => {}
        Err(e) => {
            return Err(format!("网络连通性检查失败：{}", e));
        }
    };
    let connectivity = match de_proxy.ip4_connectivity().await {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("获取网络连通状态失败：{}", e));
        }
    };
    if connectivity == ConnectivityState::Full.code() {
        tracing::debug!("网络已连通");
        return Ok(false);
    }

    // 如果是有线连接，则检查IP地址
    if ac_type == "802-3-ethernet" {
        let ic_path = match de_proxy.ip4_config().await {
            Ok(p) => p,
            Err(e) => {
                return Err(format!("获取IP配置路径失败：{}", e));
            }
        };

        let ic_proxy = match IP4ConfigProxy::builder(&conn)
            .path(ic_path)
            .unwrap()
            .build()
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return Err(format!("IP配置代理创建失败：{}", e));
            }
        };

        let address = match ic_proxy.address_data().await {
            Ok(a) => a,
            Err(e) => {
                return Err(format!("获取IP信息失败：{}", e));
            }
        };
        if address.is_empty() {
            return Err(String::from("无可用IP信息"));
        };
        let address = address[0].to_owned();
        let ip_val = address.get("address").unwrap();
        let ip = String::try_from(ip_val.clone()).unwrap();

        if ip.starts_with("114.214.") {
            return Ok(true);
        } else {
            return Ok(false);
        }
    }

    // 如果是无线连接，则检查ID
    if ac_type == "802-11-wireless" {
        let ac_id = match ac_proxy.id().await {
            Ok(t) => t,
            Err(e) => {
                return Err(format!("获取设备类型失败：{}", e));
            }
        };
        tracing::debug!("{} -> {}", primary, ac_id);

        if ac_id == "ustcnet" {
            return Ok(true);
        } else {
            return Ok(false);
        }
    }

    Err(String::from("未知的连接类型"))
}

async fn log_in() -> Result<(), String> {
    // TODO: 监听网络连接更改事件

    // 检查是否需要登录
    match need_log_in().await {
        Ok(flag) => {
            if !flag {
                tracing::info!("未检测到网络通连接，或网络通已登录");
                return Ok(());
            }
        }
        Err(e) => {
            return Err(format!("kok: {}", e));
        }
    };
    tracing::info!("检测到未登录的网络通连接，尝试登录");

    let client = match CLIENT
        .get_or_init(|| async { Client::builder().user_agent(UA).build() })
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("Error occurred while creating http client: {}", e));
        }
    };

    let (name, password) = match get_name_password().await {
        Ok(ret) => ret,
        Err(e) => {
            return Err(e);
        }
    };

    let net_type = format!("{}", 0);
    let exp = format!("{}", 0);
    let form_param = [
        ("cmd", "set"),
        ("go", " 开通网络 "),
        ("name", &name),
        ("password", &password),
        ("type", &net_type),
        ("exp", &exp),
    ];
    let respose = match client
        .get("http://wlt.ustc.edu.cn/cgi-bin/ip/")
        .form(&form_param)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return Err(format!("Error occurred while log in to WLT: {}", e));
        }
    };

    if respose.headers().contains_key("set-cookie") {
        tracing::info!("登录成功");
        Ok(())
    } else {
        Err(format!("登录失败，响应：{:?}", respose))
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

    if fs::metadata(SOCKET_FILE).is_ok() {
        fs::remove_file(SOCKET_FILE).unwrap();
    }

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
                        tracing::debug!("received: {}", String::from_utf8_lossy(&buf));

                        if &buf[..n] == b"LOGIN" || &buf[..n] == b"REFRESH" {
                            if let Err(e) = log_in().await {
                                tracing::error!("{}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error occurred while reading socket: {}", e);
                    }
                }
            }
        });
    }
}
