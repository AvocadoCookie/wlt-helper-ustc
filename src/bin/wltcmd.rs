use std::{
    collections::HashMap,
    env, fs,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use clap::{Parser, Subcommand};
use oo7::Keyring;
use tokio::sync::OnceCell;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use wlt_helper::{Config, Error, SOCKET_FILE};

static KEYRING: OnceCell<Keyring> = OnceCell::const_new();

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    op: Op,
}

#[derive(Subcommand)]
enum Op {
    Login,
    Refresh,
}

async fn store_name_password(name: String, password: String) -> Result<(), Error> {
    let mut config = Config::get_config()?;
    config.name = name.clone();
    config.store()?;

    let keyring = KEYRING.get().unwrap();
    let mut attr = HashMap::new();
    attr.insert("app", "wlt-helper");
    attr.insert("user", &name);

    keyring
        .create_item("wlt-helper password store", &attr, password, true)
        .await
        .map_err(|e| Error::Keyring(e.to_string()))?;

    Ok(())
}

#[tokio::main]
async fn main() {
    let log_path = {
        let base = env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            format!(
                "{}/.local/share",
                env::var("HOME").unwrap_or_default()
            )
        });
        format!("{}/wlt-helper", base)
    };
    let path = Path::new(&log_path);
    if !path.is_dir() {
        if path.exists() {
            eprintln!("日志路径被文件占用：{}", log_path);
            return;
        }
        if let Err(e) = fs::create_dir_all(path) {
            eprintln!("创建日志路径失败：{}", e);
        }
    }
    let (log_file, _guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(path, "client.log"));

    let env_filter = EnvFilter::new(if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(log_file).with_ansi(false))
        .init();

    KEYRING
        .get_or_init(|| async { Keyring::new().await.unwrap() })
        .await;

    let cli = Cli::parse();
    let mut stream = match UnixStream::connect(SOCKET_FILE) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("无法连接到守护进程（{}）：{}", SOCKET_FILE, e);
            return;
        }
    };

    match cli.op {
        Op::Login => {
            print!("用户名：");
            io::stdout().flush().unwrap();
            let mut name = String::new();
            if let Err(e) = io::stdin().read_line(&mut name) {
                eprintln!("用户名输入失败：{}", e);
                return;
            }
            let name = name.trim().to_string();

            let password = match rpassword::prompt_password("密码：") {
                Ok(p) => p.trim().to_string(),
                Err(e) => {
                    eprintln!("密码输入失败：{}", e);
                    return;
                }
            };

            if let Err(e) = store_name_password(name, password).await {
                eprintln!("密码存储失败：{}", e);
                tracing::error!("密码存储失败：{}", e);
                return;
            }

            stream.write_all(b"LOGIN").unwrap();
        }
        Op::Refresh => {
            stream.write_all(b"REFRESH").unwrap();
        }
    }

    if let Err(e) = stream.shutdown(std::net::Shutdown::Write) {
        eprintln!("关闭 socket 写端失败：{}", e);
        return;
    }
    let mut response = String::new();
    if let Err(e) = stream.read_to_string(&mut response) {
        eprintln!("读取守护进程响应失败：{}", e);
        return;
    }
    print!("{}", response);
}