use anyhow::{Context, Result};
use std::io::{Read, Seek};
use std::path::PathBuf;

pub(in crate::voices) fn read_icon<R: Read + Seek>(reader: R) -> Result<Vec<u8>> {
    let mut zip =
        zip::ZipArchive::new(reader).context(format!("Failed to open as zip images.dat"))?;

    let mut icon = zip
        .by_path(PathBuf::from("images/icon.png"))
        .context(format!("Failed to open images/icon.png from images.dat"))?;

    let mut file = vec![];

    icon.read_to_end(&mut file)
        .context(format!("Failed to read images/icon.png from images.dat"))?;

    Ok(file)
}
