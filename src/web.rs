use axum::{
    Router,
    extract::{Json, State},
    http::header,
    response::IntoResponse,
    routing::{get, post},
};
use base64::prelude::*;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

use crate::model::{Request, RequestContext, Voice};

#[derive(Clone)]
struct AppState {
    worker_socket: mpsc::Sender<RequestContext>,
}

async fn root_handler() -> impl IntoResponse {
    let mut html = "<h1>AITALKED SERVER</h1>\n".to_string();

    html += &format!("<p>{} models avialble</p>\n", crate::worker::get_voice_icons().len());
    html += "<ul>\n";

    for (name, icon) in crate::worker::get_voice_icons() {
        html += &format!(
            "<li><img src=\"data:image/png;base64,{}\" width=48> {name}</li>\n",
            BASE64_STANDARD.encode(icon)
        );
    }

    html += "</ul>\n";

    ([(header::CONTENT_TYPE, "text/html")], html)
}

async fn voices_handler() -> Json<Vec<Voice>> {
    Json(
        crate::worker::get_voice_icons()
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
    Json(body): Json<Request>,
) -> impl IntoResponse {
    let (tx, rx) = oneshot::channel();

    state
        .worker_socket
        .send(RequestContext { body, channel: tx })
        .await
        .unwrap();

    match rx.await.unwrap() {
        Ok(voice) => ([(header::CONTENT_TYPE, "application/octet-stream")], voice),
        Err(e) => (
            [(header::CONTENT_TYPE, "text/plain")],
            e.to_string().into_bytes(),
        ),
    }
}

pub async fn serve(
    listener: TcpListener,
    worker_socket: mpsc::Sender<RequestContext>,
) -> Result<(), std::io::Error> {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/api/tts", post(tts_handler))
        .route("/api/voices", get(voices_handler))
        .with_state(AppState { worker_socket });

    axum::serve(listener, app).await
}
