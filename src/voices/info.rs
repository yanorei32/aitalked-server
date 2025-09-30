use std::io::{Cursor, Read, Seek};

use aes::Aes128;
use aes::cipher::{block_padding::Pkcs7, generic_array::GenericArray};
use cbc::Decryptor;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use flate2::read::ZlibDecoder;
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn to_hex_string(&self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StyleDefinition {
    pub name: String,
    pub display_name: String,
    pub color: Color,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Styles {
    #[serde(rename = "StyleDefinition")]
    style_definitions: Option<Vec<StyleDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VoiceDicInfo {
    pub format: String,
    pub samples_per_sec: u32,
    pub language: String,
    pub dialect: String,
    pub name: String,
    pub gender: String,
    pub background_color: Color,
    pub styles: Styles,
    #[serde(rename = "FeatureID")]
    pub feature_id: u32,
    pub hash_code_string: String,
    pub version_string: String,
    #[serde(rename = "AITalkVersionString")]
    pub ai_talk_version_string: String,
    #[serde(rename = "NGWords")]
    pub ng_words: Vec<String>,
}

fn aes_decrypt(key: &[u8], iv: &[u8], data: &mut [u8]) -> Vec<u8> {
    let decryptor = Decryptor::<Aes128>::new_from_slices(key, iv).expect("invalid key/iv length");

    decryptor
        .decrypt_padded_mut::<Pkcs7>(data)
        .expect("decryption error")
        .to_vec()
}

pub(in crate::voices) fn read_info<R: Read + Seek>(mut reader: R) -> Result<VoiceDicInfo> {
    let password = b"jD5yPFM63olaOWC5fiGpLL5LJnpwTlsK";

    let mut salt = [0; 16];
    let mut iv = [0; 16];

    reader.read_exact(&mut salt)?;
    reader.read_exact(&mut iv)?;

    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password, &salt, 1000, &mut key);

    let key = GenericArray::from(key);

    let mut body = vec![];

    reader.read_to_end(&mut body)?;

    let compressed = aes_decrypt(&key, &iv, &mut body);

    let mut zlib = ZlibDecoder::new_with_decompress(
        Cursor::new(compressed),
        flate2::Decompress::new_with_window_bits(false, 15),
    );

    let mut s = String::new();

    zlib.read_to_string(&mut s)?;

    Ok(serde_xml_rs::from_str(&s)?)
}
