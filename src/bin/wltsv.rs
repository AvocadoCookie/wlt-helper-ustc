use std::{collections::HashMap, fs};

use oo7::{Keyring, Secret};
use reqwest::Client;
use tokio::{io::AsyncReadExt, net::UnixListener, sync::OnceCell};
use tracing::Level;
use wlt_helper::{Config, SOCKET_FILE};

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

async fn log_in() -> Result<(), String> {
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

    tracing::info!("{:?}", respose);

    Ok(())
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
