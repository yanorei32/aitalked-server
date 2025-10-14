use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;

pub mod images;
pub mod info;

static VOICES: OnceCell<HashMap<String, (Vec<u8>, info::VoiceDicInfo)>> = OnceCell::new();

pub fn get() -> &'static HashMap<String, (Vec<u8>, info::VoiceDicInfo)> {
    VOICES.get().unwrap()
}

fn find_voice_dbs(dir_voice_dbs: &Path) -> Result<Vec<String>> {
    Ok(std::fs::read_dir(dir_voice_dbs)
        .context("Failed to read VoiceDB Directory")?
        .map(|entry| entry.unwrap())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .collect())
}

fn open_icon(installation_dir: &Path, voice_name: &str) -> Result<Vec<u8>> {
    let path = installation_dir
        .join("Voice")
        .join(voice_name)
        .join("images");

    if path.is_dir() {
        let mut f = File::open(path.join("icon.png"))
            .context(format!("Failed to open {voice_name}'s icon.png"))?;

        let mut data = vec![];
        f.read_to_end(&mut data)
            .context(format!("Failed to read {voice_name}'s icon.png"))?;

        return Ok(data);
    }

    let path = installation_dir
        .join("Voice")
        .join(voice_name)
        .join("images.dat");

    let f = File::open(path).context(format!("Failed to open {voice_name}'s images.dat"))?;

    images::read_icon(f).context(format!("Failed to read {voice_name}'s images.dat"))
}

fn open_info(
    installation_dir: &Path,
    voice_name: &str,
    password: &str,
) -> Result<info::VoiceDicInfo> {
    let path = installation_dir
        .join("Voice")
        .join(voice_name)
        .join("info.bin");

    let f = File::open(path).context(format!("Failed to open {voice_name}'s info.bin"))?;

    info::read_info(f, password).context(format!("Failed to read {voice_name}'s info.bin"))
}

pub fn init(installation_dir: &Path, infobin_password: &str) -> Result<()> {
    let voices: Result<HashMap<_, _>> = find_voice_dbs(&installation_dir.join("Voice"))
        .unwrap()
        .iter()
        .map(|name| {
            let info = open_info(installation_dir, name, infobin_password)?;
            let icon = open_icon(installation_dir, name)?;
            Ok((name.clone(), (icon, info)))
        })
        .collect();

    let vocies = voices?;

    VOICES.get_or_init(|| vocies);
    tracing::info!("Voices Ready");

    Ok(())
}
