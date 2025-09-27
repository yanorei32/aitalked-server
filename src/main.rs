use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::{mpsc, oneshot};

mod icon;
mod model;
mod web;
mod worker;

#[derive(Debug, Parser, Clone)]
struct Cli {
    #[arg(long, env, default_value = "C:\\Program Files (x86)\\AHS\\VOICEROID2")]
    installation_dir: PathBuf,

    #[arg(long, env, default_value = "ORXJC6AIWAUKDpDbH2al")]
    auth_seed: String,

    #[clap(long, env)]
    #[clap(default_value = "0.0.0.0:3000")]
    listen: SocketAddr,

    #[arg(long, env)]
    word_dic: Option<PathBuf>,

    #[arg(long, env)]
    phrase_dic: Option<PathBuf>,

    #[arg(long, env)]
    symbol_dic: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_thread_names(true).init();

    let cli = Cli::parse();
    std::env::set_current_dir(&cli.installation_dir).unwrap();

    let listen = cli.listen;

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("Failed to bind address {listen}"))?;

    let (tx_kansai, rx_kansai) = mpsc::channel(1);
    let (tx_kansai_result, rx_kansai_result) = oneshot::channel();
    let (tx, rx) = mpsc::channel(1);
    let (tx_result, rx_result) = oneshot::channel();
    let (tx_icon_result, rx_icon_result) = oneshot::channel();

    std::thread::Builder::new()
        .name("WkrStdKnsi".to_string())
        .spawn({
            let cli = cli.clone();
            move || match worker::initialization(
                &cli.installation_dir,
                "aitalked_kansai.dll",
                "Lang\\standard_kansai",
                cli.word_dic.as_deref(),
                cli.phrase_dic.as_deref(),
                cli.symbol_dic.as_deref(),
                &cli.auth_seed,
            ) {
                Ok((aitalked, param)) => {
                    tx_kansai_result.send(Ok(())).unwrap();
                    worker::event_loop(aitalked, param, rx_kansai);
                }
                Err(e) => {
                    tx_kansai_result.send(Err(e)).unwrap();
                }
            }
        })
        .unwrap();

    std::thread::Builder::new()
        .name("WkrStd".to_string())
        .spawn({
            let cli = cli.clone();
            move || match worker::initialization(
                &cli.installation_dir,
                "aitalked.dll",
                "Lang\\standard",
                cli.word_dic.as_deref(),
                cli.phrase_dic.as_deref(),
                cli.symbol_dic.as_deref(),
                &cli.auth_seed,
            ) {
                Ok((aitalked, param)) => {
                    tx_result.send(Ok(())).unwrap();
                    worker::event_loop(aitalked, param, rx);
                }
                Err(e) => {
                    tx_result.send(Err(e)).unwrap();
                }
            }
        })
        .unwrap();

    std::thread::Builder::new()
        .name("InitIcon".to_string())
        .spawn({
            let cli = cli.clone();
            move || {
                tx_icon_result
                    .send(icon::init(&cli.installation_dir))
                    .unwrap();
            }
        })
        .unwrap();

    rx_icon_result.await.unwrap().expect("Failed to init icon");

    rx_result
        .await
        .unwrap()
        .expect("Failed to init worker standard");

    rx_kansai_result
        .await
        .unwrap()
        .expect("Failed to init worker standard_kansai");

    tracing::info!("Ready to use");
    web::serve(listener, tx, tx_kansai).await.unwrap();

    Ok(())
}
