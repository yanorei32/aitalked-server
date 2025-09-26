use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::io::Read;
use std::path::{Path, PathBuf};

use aitalked::{api as aitalked_api, binding::*, model::*};
use anyhow::{Context, Result, anyhow};
use encoding_rs::SHIFT_JIS;
use once_cell::sync::OnceCell;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::model::RequestContext;

static VOICE_ICONS: OnceCell<HashMap<String, Vec<u8>>> = OnceCell::new();

pub fn get_voice_icons() -> &'static HashMap<String, Vec<u8>> {
    VOICE_ICONS.get().unwrap()
}

fn path_to_sjis_cstring(path: &Path) -> CString {
    CString::new(SHIFT_JIS.encode(path.to_str().unwrap()).0).unwrap()
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

fn read_icon(dir_voice_dbs: &Path, voice_name: &str) -> Result<Vec<u8>> {
    let path = dir_voice_dbs.join(voice_name).join("images.dat");

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

pub async fn initialization(
    installation_dir: &Path,
    word_dic: Option<&Path>,
    phrase_dic: Option<&Path>,
    symbol_dic: Option<&Path>,
    auth_seed: &str,
) -> Result<()> {
    unsafe { aitalked::load_dll(&installation_dir.join("aitalked.dll")) }
        .context("Failed to initialization aitalked.dll")?;

    let dir_voice_dbs = installation_dir.join("Voice");
    let dir_voice_dbs_sjis = path_to_sjis_cstring(&dir_voice_dbs);
    let path_license_sjis = path_to_sjis_cstring(&installation_dir.join("aitalk.lic"));
    let auth_seed = CString::new(auth_seed).unwrap();

    let config = AitalkedConfig {
        hz_voice_db: 44100,
        dir_voice_dbs: dir_voice_dbs_sjis.as_ptr(),
        msec_timeout: 1000,
        path_license: path_license_sjis.as_ptr(),
        code_auth_seed: auth_seed.as_ptr(),
        len_auth_seed: 0,
    };

    let code = unsafe { aitalked_api::init(&config) };

    if code != ResultCode::SUCCESS {
        anyhow::bail!("Failed to aitalked_api::init {code:?}");
    }

    if let Some(word_dic) = word_dic {
        let code = unsafe { aitalked_api::reload_word_dic(Some(&path_to_sjis_cstring(word_dic))) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked_api::reload_word_dic {code:?}");
        }
    }

    if let Some(phrase_dic) = phrase_dic {
        let code =
            unsafe { aitalked_api::reload_phrase_dic(Some(&path_to_sjis_cstring(phrase_dic))) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked_api::reload_phrase_dic {code:?}");
        }
    }

    if let Some(symbol_dic) = symbol_dic {
        let code =
            unsafe { aitalked_api::reload_symbol_dic(Some(&path_to_sjis_cstring(symbol_dic))) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked_api::reload_symbol_dic {code:?}");
        }
    }

    let voice_names = find_voice_dbs(&dir_voice_dbs).unwrap();
    let mut voice_icons = HashMap::new();

    for name in voice_names {
        tracing::info!("Initializing {name}...");

        let code =
            unsafe { aitalked_api::voice_load(&CString::new(SHIFT_JIS.encode(&name).0).unwrap()) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked_api::voice_load {code:?}");
        }

        let icon = read_icon(&dir_voice_dbs, &name)?;
        voice_icons.insert(name.clone(), icon);
    }

    VOICE_ICONS.set(voice_icons).unwrap();

    Ok(())
}

struct TextToSpeechContext<'a> {
    buffer: &'a mut Vec<u8>,
    notify: mpsc::Sender<()>,
    len_raw_buf_words: u32,
}

extern "system" fn tts_event_callback(
    _reason_code: EventReasonCode,
    _job_id: i32,
    _tick: u64,
    _name: *const c_char,
    _user_data: *mut c_void,
) -> i32 {
    0
}

extern "system" fn raw_buf_callback(
    reason_code: EventReasonCode,
    job_id: i32,
    _tick: u64,
    user_data: *mut c_void,
) -> i32 {
    match reason_code {
        EventReasonCode::RAWBUF_FULL
        | EventReasonCode::RAWBUF_FLUSH
        | EventReasonCode::RAWBUF_CLOSE => (),
        _ => return 0,
    }

    let context = unsafe { &mut *(user_data as *mut TextToSpeechContext<'static>) };
    let buffer_bytes = (context.len_raw_buf_words * 2).min(LEN_RAW_BUF_MAX_BYTES);

    let mut buffer = vec![0; buffer_bytes as usize];

    loop {
        let mut samples_read = 0;
        let code = unsafe { aitalked_api::get_data(job_id, &mut buffer, &mut samples_read) };

        if code != ResultCode::SUCCESS {
            break;
        }

        context
            .buffer
            .extend_from_slice(&buffer[0..(samples_read * 2) as usize]);

        if samples_read * 2 < buffer_bytes {
            break;
        }
    }

    if reason_code == EventReasonCode::RAWBUF_CLOSE {
        context.notify.blocking_send(()).unwrap();
    }

    0
}

struct ProcTextBufContext<'a> {
    buffer: &'a mut Vec<u8>,
    notify: mpsc::Sender<()>,
    len_text_buf_bytes: u32,
}

extern "system" fn text_buffer_callback(
    reason_code: EventReasonCode,
    job_id: i32,
    user_data: *mut c_void,
) -> i32 {
    match reason_code {
        EventReasonCode::TEXTBUF_FULL
        | EventReasonCode::TEXTBUF_FLUSH
        | EventReasonCode::TEXTBUF_CLOSE => (),
        _ => return 0,
    }

    let context = unsafe { &mut *(user_data as *mut ProcTextBufContext<'static>) };
    let buffer_length = context.len_text_buf_bytes.min(LEN_TEXT_BUF_MAX);

    let mut buffer = vec![0; buffer_length as usize];

    loop {
        let mut bytes_read = 0;
        let mut position = 0;

        let code =
            unsafe { aitalked_api::get_kana(job_id, &mut buffer, &mut bytes_read, &mut position) };

        if code != ResultCode::SUCCESS {
            break;
        }

        context
            .buffer
            .extend_from_slice(&buffer[0..bytes_read as usize]);

        if bytes_read < buffer_length - 1 {
            break;
        }
    }

    if reason_code == EventReasonCode::TEXTBUF_CLOSE {
        context.notify.blocking_send(()).unwrap();
    }

    0
}

fn voicename_to_buffer(s: &str) -> [c_char; MAX_VOICE_NAME] {
    let mut buffer = [0 as c_char; MAX_VOICE_NAME];

    buffer
        .iter_mut()
        .zip(SHIFT_JIS.encode(s).0.iter())
        .for_each(|(dest, src)| {
            *dest = *src as c_char;
        });

    buffer
}

pub async fn event_loop(mut rx: mpsc::Receiver<RequestContext>) -> Result<()> {
    let empty_tts_param_size = std::mem::size_of::<TtsParam>() as u32;
    let speaker_param_size = std::mem::size_of::<SpeakerParam>() as u32;
    let mut actual_tts_param_size = 0;

    let code = unsafe { aitalked_api::get_param(std::ptr::null_mut(), &mut actual_tts_param_size) };

    if code != ResultCode::INSUFFICIENT {
        anyhow::bail!("Failed to aitalked_api::get_param (size query) {code:?}");
    }

    let estimate_speaker_param_count =
        (actual_tts_param_size - empty_tts_param_size) / speaker_param_size;

    let mut boxed_tts_param = BoxedTtsParam::new(estimate_speaker_param_count as usize);

    let code = unsafe {
        aitalked_api::get_param(boxed_tts_param.tts_param_mut(), &mut actual_tts_param_size)
    };

    if code != ResultCode::SUCCESS {
        anyhow::bail!("Failed to aitalked_api::get_param (size query) {code:?}");
    }

    let mut loaded_lang = None;

    loop {
        let ctx = rx.recv().await.unwrap();

        let t_start_at = Instant::now();

        /*\
        |*| Parameter Initialization
        \*/
        let voice_name = voicename_to_buffer(&ctx.body.voice_name);

        let Some(speaker) = boxed_tts_param
            .speakers_mut()
            .iter_mut()
            .find(|s| s.voice_name == voice_name)
        else {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to find speaker from tts_param {}",
                    ctx.body.voice_name
                )))
                .unwrap();

            continue;
        };

        speaker.volume = ctx.body.volume;
        speaker.speed = ctx.body.speed;
        speaker.pitch = ctx.body.pitch;
        speaker.range = ctx.body.range;
        speaker.pause_middle = ctx.body.pause_middle;
        speaker.pause_long = ctx.body.pause_long;
        speaker.pause_sentence = ctx.body.pause_sentence;
        boxed_tts_param.tts_param_mut().voice_name = speaker.voice_name;
        boxed_tts_param.tts_param_mut().proc_text_buf = None;
        boxed_tts_param.tts_param_mut().proc_raw_buf = None;
        boxed_tts_param.tts_param_mut().proc_event_tts = None;

        /*\
        |*| Lang unload / load
        \*/
        let lang = if ctx
            .body
            .is_kansai
            .unwrap_or(ctx.body.voice_name.contains("west"))
        {
            CString::new("Lang\\standard_kansai").unwrap()
        } else {
            CString::new("Lang\\standard").unwrap()
        };


        let reload_required = match loaded_lang {
            Some(ref loaded_lang) => loaded_lang != &lang,
            None => true,
        };

        if reload_required {
            let code = unsafe { aitalked_api::lang_clear() };
            if code != ResultCode::SUCCESS && code != ResultCode::NOT_LOADED {
                ctx.channel
                    .send(Err(anyhow!("Failed to aitalked_api::lang_clear {code:?}")))
                    .unwrap();

                continue;
            }

            let code = unsafe { aitalked_api::lang_load(&lang) };
            if code != ResultCode::SUCCESS {
                ctx.channel
                    .send(Err(anyhow!(
                        "Failed to aitalked_api::lang_load {lang:?} {code:?}"
                    )))
                    .unwrap();

                continue;
            }

            loaded_lang = Some(lang.clone());
        }

        let t_lang_ready = Instant::now();

        /*\
        |*| Start Text2Kana
        \*/
        boxed_tts_param.tts_param_mut().proc_text_buf = Some(text_buffer_callback);

        let code = unsafe { aitalked_api::set_param(boxed_tts_param.tts_param()) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked_api::set_param (text_to_kana) {code:?}"
                )))
                .unwrap();

            continue;
        }

        let mut job_id = 0;

        let mut kana = vec![];
        let (tx, mut rx) = mpsc::channel(1);

        let mut context = ProcTextBufContext {
            buffer: &mut kana,
            notify: tx.clone(),
            len_text_buf_bytes: boxed_tts_param.tts_param().len_text_buf_bytes,
        };

        let code = unsafe {
            aitalked_api::text_to_kana(
                &mut job_id,
                &mut context as *mut ProcTextBufContext as *mut std::ffi::c_void,
                &CString::new(SHIFT_JIS.encode(&ctx.body.text).0).unwrap(),
            )
        };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked_api::text_to_kana {code:?}"
                )))
                .unwrap();

            continue;
        }

        // await EOF received
        rx.recv().await.unwrap();

        drop(context);

        let code = unsafe { aitalked_api::close_kana(job_id, 0) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!("Failed to aitalked_api::close_kana {code:?}")))
                .unwrap();

            continue;
        }

        // Add '\0'
        kana.push(0);

        let kana = CStr::from_bytes_with_nul(&kana).unwrap();

        // unload
        boxed_tts_param.tts_param_mut().proc_text_buf = None;

        let t_kana_ready = Instant::now();

        /*\
        |*| Start Kana2Speech
        \*/
        boxed_tts_param.tts_param_mut().proc_raw_buf = Some(raw_buf_callback);
        boxed_tts_param.tts_param_mut().proc_event_tts = Some(tts_event_callback);
        let code = unsafe { aitalked_api::set_param(boxed_tts_param.tts_param()) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked_api::set_param (kana_to_speech / set) {code:?}"
                )))
                .unwrap();

            continue;
        }

        let mut job_id = 0;
        let (tx, mut rx) = mpsc::channel(1);

        let mut buffer = vec![];

        let mut context = TextToSpeechContext {
            buffer: &mut buffer,
            notify: tx.clone(),
            len_raw_buf_words: boxed_tts_param.tts_param().len_raw_buf_words,
        };

        let code = unsafe {
            aitalked_api::text_to_speech(
                &mut job_id,
                &mut context as *mut TextToSpeechContext as *mut std::ffi::c_void,
                kana,
            )
        };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked_api::text_to_speech {code:?}"
                )))
                .unwrap();

            continue;
        }

        // await EOF received
        rx.recv().await.unwrap();

        drop(context);

        let code = unsafe { aitalked_api::close_speech(job_id, 0) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked_api::close_speech {code:?}"
                )))
                .unwrap();

            continue;
        }

        let t_speech_ready = Instant::now();

        tracing::info!(
            "Lang: {:?}, Kana: {:?}, Speech: {:?}",
            t_lang_ready - t_start_at,
            t_kana_ready - t_lang_ready,
            t_speech_ready - t_kana_ready,
        );

        ctx.channel.send(Ok(buffer)).unwrap();
    }
}
