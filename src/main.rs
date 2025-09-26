use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::{mpsc, oneshot};

pub mod model;
mod web;
mod worker;

#[derive(Debug, Parser)]
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

    let (tx, rx) = mpsc::channel(1);

    let (err_tx, err_rx) = oneshot::channel();

    worker::initialization(
        &cli.installation_dir,
        cli.word_dic.as_deref(),
        cli.phrase_dic.as_deref(),
        cli.symbol_dic.as_deref(),
        &cli.auth_seed,
    )
    .await
    .unwrap();


    tracing::info!("Ready");

    tokio::spawn(async move {
        err_tx.send(web::serve(listener, tx).await).unwrap();
    });

    tokio::select! {
        result = worker::event_loop(rx) => {
            result?;
        },
        result = err_rx => {
            result.unwrap()?;
        },
    }

    Ok(())
}
