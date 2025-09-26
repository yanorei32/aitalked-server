use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::mpsc;

mod model;
mod icon;
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    std::env::set_current_dir(&cli.installation_dir).unwrap();

    let listen = cli.listen;

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("Failed to bind address {listen}"))?;

    let (tx_kansai, rx_kansai) = mpsc::channel(1);
    let (tx, rx) = mpsc::channel(1);

    std::thread::spawn({
        let cli = cli.clone();
        move || {
            icon::init(&cli.installation_dir).unwrap();
        }
    });

    std::thread::spawn({
        let cli = cli.clone();
        move || {
            worker::event_loop(
                &cli.installation_dir,
                "aitalked_kansai.dll",
                "Lang\\standard_kansai",
                cli.word_dic.as_deref(),
                cli.phrase_dic.as_deref(),
                cli.symbol_dic.as_deref(),
                &cli.auth_seed,
                rx_kansai,
            )
            .unwrap();
        }
    });

    std::thread::spawn({
        let cli = cli.clone();
        move || {
            worker::event_loop(
                &cli.installation_dir,
                "aitalked.dll",
                "Lang\\standard",
                cli.word_dic.as_deref(),
                cli.phrase_dic.as_deref(),
                cli.symbol_dic.as_deref(),
                &cli.auth_seed,
                rx,
            )
            .unwrap();
        }
    });

    tracing::info!("Ready");

    web::serve(listener, tx, tx_kansai).await.unwrap();

    Ok(())
}
