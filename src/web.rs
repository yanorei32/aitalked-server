use axum::{
    Router,
    extract::{Json, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use base64::prelude::*;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

use crate::model::{ApiRequest, RequestContext, Voice};

#[derive(Clone)]
struct AppState {
    worker_socket: mpsc::Sender<RequestContext>,
    worker_socket_kansai: mpsc::Sender<RequestContext>,
}

async fn root_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html")], include_str!("../assets/index.html"))
}

async fn voices_handler() -> Json<Vec<Voice>> {
    Json(
        crate::icon::get()
            .iter()
            .map(|(name, icon)| Voice {
                name: name.to_string(),
                icon: BASE64_STANDARD.encode(icon),
            })
            .collect::<Vec<_>>(),
    )
}

async fn tts_handler(
    State(state): State<AppState>,
    Json(api_req): Json<ApiRequest>,
) -> impl IntoResponse {
    let (tx, rx) = oneshot::channel();

    let worker = if api_req
        .is_kansai
        .unwrap_or(api_req.body.voice_name.contains("west"))
    {
        state.worker_socket_kansai
    } else {
        state.worker_socket
    };

    worker
        .send(RequestContext {
            body: api_req.body,
            channel: tx,
        })
        .await
        .unwrap();

    match rx.await.unwrap() {
        Ok(voice) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            voice,
        ),
        Err(e) => {
            tracing::warn!("{e}");
            (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "text/plain")],
                e.to_string().into_bytes(),
            )
        }
    }
}

pub async fn serve(
    listener: TcpListener,
    worker_socket: mpsc::Sender<RequestContext>,
    worker_socket_kansai: mpsc::Sender<RequestContext>,
) -> Result<(), std::io::Error> {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/api/tts", post(tts_handler))
        .route("/api/voices", get(voices_handler))
        .with_state(AppState {
            worker_socket,
            worker_socket_kansai,
        });

    axum::serve(listener, app).await
}
