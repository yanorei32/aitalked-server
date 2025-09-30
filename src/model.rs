use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

fn default_pause_sentence() -> i32 {
    800
}

fn default_pause_long() -> i32 {
    370
}

fn default_pause_middle() -> i32 {
    150
}

fn default_volume() -> f32 {
    1.0
}

fn default_speed() -> f32 {
    1.0
}

fn default_pitch() -> f32 {
    1.0
}

fn default_range() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRequest {
    pub is_kansai: Option<bool>,

    #[serde(flatten)]
    pub body: Request,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub voice_id: String,
    pub text: String,

    #[serde(default = "default_volume")]
    pub volume: f32,

    #[serde(default = "default_speed")]
    pub speed: f32,

    #[serde(default = "default_pitch")]
    pub pitch: f32,

    #[serde(default = "default_range")]
    pub range: f32,

    #[serde(default = "default_pause_middle")]
    pub pause_middle: i32,

    #[serde(default = "default_pause_long")]
    pub pause_long: i32,

    #[serde(default = "default_pause_sentence")]
    pub pause_sentence: i32,
}

#[derive(Debug)]
pub struct RequestContext {
    pub body: Request,
    pub channel: oneshot::Sender<Result<Vec<u8>>>,
}

#[derive(Debug, Serialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub dialect: String,
    pub gender: String,
    pub background_color: String,
}
