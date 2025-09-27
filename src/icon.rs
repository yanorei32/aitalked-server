use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::io::Read;

use anyhow::{Context, Result};

use once_cell::sync::OnceCell;

static ICONS: OnceCell<HashMap<String, Vec<u8>>> = OnceCell::new();

pub fn get() -> &'static HashMap<String, Vec<u8>> {
    ICONS.get().unwrap()
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

fn read(installation_dir: &Path, voice_name: &str) -> Result<Vec<u8>> {
    let path = installation_dir
        .join("Voice")
        .join(voice_name)
        .join("images.dat");

    let zip =
        std::fs::File::open(path).context(format!("Failed to open {voice_name}'s images.dat"))?;

    let mut zip = zip::ZipArchive::new(zip)
        .context(format!("Failed to open as zip {voice_name}'s images.dat"))?;

    let mut icon = zip
        .by_path(PathBuf::from("images/icon.png"))
        .context(format!(
            "Failed to open images/icon.png from {voice_name}/images.dat"
        ))?;

    let mut file = vec![];

    icon.read_to_end(&mut file).context(format!(
        "Failed to read images/icon.png from {voice_name}/images.dat"
    ))?;

    Ok(file)
}

pub fn init(installation_dir: &Path) -> Result<()> {
    let icons: Result<HashMap<_, _>> = find_voice_dbs(&installation_dir.join("Voice"))
        .unwrap()
        .iter()
        .map(|name| read(installation_dir, name).map(|v| (name.clone(), v)))
        .collect();

    let icons = icons?;

    ICONS.get_or_init(|| icons);
    tracing::info!("Icon Ready");

    Ok(())
}

