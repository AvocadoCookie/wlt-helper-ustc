use std::{
    collections::HashMap,
    env, fs,
    io::{self, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use clap::{Parser, Subcommand};
use oo7::Keyring;
use tokio::sync::OnceCell;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use wlt_helper::{Config, SOCKET_FILE};

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

async fn store_name_password(name: String, password: String) -> Result<(), String> {
    let mut config = match Config::get_config() {
        Ok(c) => c,
        Err(e) => {
            return Err(e);
        }
    };
    config.name = name.clone();
    if let Err(e) = config.store() {
        println!("保存配置文件失败");
        return Err(e);
    }

    let keyring = KEYRING.get().unwrap();
    let mut attr = HashMap::new();
    attr.insert("app", "wlt-helper");
    attr.insert("user", &name);

    match keyring
        .create_item("wlt-helper password store", &attr, password, true)
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Error occurred while storing password: {}", e)),
    }
}

#[tokio::main]
async fn main() {
    let log_path = format!(
        "/home/{}/.local/share/wlt-helper",
        env::var("USER").unwrap()
    );
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
        tracing_appender::non_blocking(tracing_appender::rolling::never(path, "client.log"));

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
            eprintln!("Can't connect to socket file {}: {}", SOCKET_FILE, e);
            return;
        }
    };

    match cli.op {
        Op::Login => {
            print!("用户名：");
            io::stdout().flush().unwrap();
            let mut name = String::new();
            if let Err(e) = io::stdin().read_line(&mut name) {
                println!("用户名输入失败");
                tracing::error!("用户名输入失败：{}", e);
                return;
            }
            let name = name.trim().to_string();

            let password = match rpassword::prompt_password("密码：") {
                Ok(p) => p.trim().to_string(),
                Err(e) => {
                    println!("密码输入失败");
                    tracing::error!("密码输入失败：{}", e);
                    return;
                }
            };

            if let Err(e) = store_name_password(name, password).await {
                println!("密码存储失败");
                tracing::error!("密码存储失败：{}", e);
                return;
            }

            stream.write_all(b"LOGIN").unwrap();
        }
        Op::Refresh => {
            stream.write_all(b"REFRESH").unwrap();
        }
    }
}
