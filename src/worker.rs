use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;
use std::io::{Write, Cursor};

use aitalked::{api::Aitalked, binding::*, model::*};
use anyhow::{Context, Result, anyhow};
use encoding_rs::SHIFT_JIS;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::model::RequestContext;

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

struct TextToSpeechContext<'a> {
    aitalked: Aitalked,
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
        let code = unsafe {
            context
                .aitalked
                .get_data(job_id, &mut buffer, &mut samples_read)
        };

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
    aitalked: Aitalked,
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

        let code = unsafe {
            context
                .aitalked
                .get_kana(job_id, &mut buffer, &mut bytes_read, &mut position)
        };

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

pub fn initialization(
    installation_dir: &Path,
    dll_name: &str,
    lang: &str,
    word_dic: Option<&Path>,
    phrase_dic: Option<&Path>,
    symbol_dic: Option<&Path>,
    auth_seed: &str,
) -> Result<(Aitalked, BoxedTtsParam)> {
    let aitalked = unsafe { aitalked::load_dll(&installation_dir.join(dll_name)) }
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

    let code = unsafe { aitalked.init(&config) };

    if code != ResultCode::SUCCESS {
        anyhow::bail!("Failed to aitalked.init {code:?}");
    }

    if let Some(word_dic) = word_dic {
        let code = unsafe { aitalked.reload_word_dic(Some(&path_to_sjis_cstring(word_dic))) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked.reload_word_dic {code:?}");
        }
    }

    if let Some(phrase_dic) = phrase_dic {
        let code = unsafe { aitalked.reload_phrase_dic(Some(&path_to_sjis_cstring(phrase_dic))) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked.reload_phrase_dic {code:?}");
        }
    }

    if let Some(symbol_dic) = symbol_dic {
        let code = unsafe { aitalked.reload_symbol_dic(Some(&path_to_sjis_cstring(symbol_dic))) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked.reload_symbol_dic {code:?}");
        }
    }

    let voice_names = find_voice_dbs(&dir_voice_dbs).unwrap();

    for name in voice_names {
        tracing::info!("Initializing {name}...");

        let code =
            unsafe { aitalked.voice_load(&CString::new(SHIFT_JIS.encode(&name).0).unwrap()) };

        if code != ResultCode::SUCCESS {
            anyhow::bail!("Failed to aitalked.voice_load {code:?}");
        }
    }

    let empty_tts_param_size = std::mem::size_of::<TtsParam>() as u32;
    let speaker_param_size = std::mem::size_of::<SpeakerParam>() as u32;
    let mut actual_tts_param_size = 0;

    let code = unsafe { aitalked.get_param(std::ptr::null_mut(), &mut actual_tts_param_size) };

    if code != ResultCode::INSUFFICIENT {
        anyhow::bail!("Failed to aitalked.get_param (size query) {code:?}");
    }

    let estimate_speaker_param_count =
        (actual_tts_param_size - empty_tts_param_size) / speaker_param_size;

    let mut boxed_tts_param = BoxedTtsParam::new(estimate_speaker_param_count as usize);

    let code =
        unsafe { aitalked.get_param(boxed_tts_param.tts_param_mut(), &mut actual_tts_param_size) };

    if code != ResultCode::SUCCESS {
        anyhow::bail!("Failed to aitalked.get_param (size query) {code:?}");
    }

    let code = unsafe { aitalked.lang_load(&CString::new(lang).unwrap()) };
    if code != ResultCode::SUCCESS {
        anyhow::bail!("Failed to aitalked.lang_load {lang} {code:?}")
    }

    Ok((aitalked, boxed_tts_param))
}

pub fn event_loop(
    aitalked: Aitalked,
    mut boxed_tts_param: BoxedTtsParam,
    mut rx: mpsc::Receiver<RequestContext>,
) {
    loop {
        let ctx = rx.blocking_recv().unwrap();

        let t_start_at = Instant::now();

        /*\
        |*| Parameter Initialization
        \*/
        let voice_name = &ctx.body.voice_name;
        let voice_name_buff = voicename_to_buffer(voice_name);

        let Some(speaker) = boxed_tts_param
            .speakers_mut()
            .iter_mut()
            .find(|s| s.voice_name == voice_name_buff)
        else {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to find speaker from tts_param {}",
                    ctx.body.voice_name
                )))
                .unwrap();

            continue;
        };

        speaker.speed = ctx.body.speed;
        speaker.pitch = ctx.body.pitch;
        speaker.range = ctx.body.range;
        speaker.pause_middle = ctx.body.pause_middle;
        speaker.pause_long = ctx.body.pause_long;
        speaker.pause_sentence = ctx.body.pause_sentence;
        boxed_tts_param.tts_param_mut().voice_name = speaker.voice_name;
        boxed_tts_param.tts_param_mut().volume = ctx.body.volume;
        boxed_tts_param.tts_param_mut().proc_text_buf = None;
        boxed_tts_param.tts_param_mut().proc_raw_buf = None;
        boxed_tts_param.tts_param_mut().proc_event_tts = None;

        /*\
        |*| Start Text2Kana
        \*/
        boxed_tts_param.tts_param_mut().proc_text_buf = Some(text_buffer_callback);

        let code = unsafe { aitalked.set_param(boxed_tts_param.tts_param()) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked.set_param (text_to_kana) {code:?}"
                )))
                .unwrap();

            continue;
        }

        let mut job_id = 0;

        let mut kana = vec![];
        let (tx, mut rx) = mpsc::channel(1);

        let mut context = ProcTextBufContext {
            aitalked,
            buffer: &mut kana,
            notify: tx.clone(),
            len_text_buf_bytes: boxed_tts_param.tts_param().len_text_buf_bytes,
        };

        let code = unsafe {
            aitalked.text_to_kana(
                &mut job_id,
                &mut context as *mut ProcTextBufContext as *mut std::ffi::c_void,
                &CString::new(SHIFT_JIS.encode(&ctx.body.text).0).unwrap(),
            )
        };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!("Failed to aitalked.text_to_kana {code:?}")))
                .unwrap();

            continue;
        }

        rx.blocking_recv().unwrap();

        drop(context);

        let code = unsafe { aitalked.close_kana(job_id, 0) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!("Failed to aitalked.close_kana {code:?}")))
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
        let code = unsafe { aitalked.set_param(boxed_tts_param.tts_param()) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!(
                    "Failed to aitalked.set_param (kana_to_speech / set) {code:?}"
                )))
                .unwrap();

            continue;
        }

        let mut job_id = 0;
        let (tx, mut rx) = mpsc::channel(1);

        const WAV_HEADER_SIZE: usize = 44;
        let mut buffer = vec![0; WAV_HEADER_SIZE];

        let mut context = TextToSpeechContext {
            aitalked,
            buffer: &mut buffer,
            notify: tx.clone(),
            len_raw_buf_words: boxed_tts_param.tts_param().len_raw_buf_words,
        };

        let code = unsafe {
            aitalked.text_to_speech(
                &mut job_id,
                &mut context as *mut TextToSpeechContext as *mut std::ffi::c_void,
                kana,
            )
        };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!("Failed to aitalked.text_to_speech {code:?}")))
                .unwrap();

            continue;
        }

        rx.blocking_recv().unwrap();

        drop(context);

        let code = unsafe { aitalked.close_speech(job_id, 0) };
        if code != ResultCode::SUCCESS {
            ctx.channel
                .send(Err(anyhow!("Failed to aitalked.close_speech {code:?}")))
                .unwrap();

            continue;
        }

        let t_speech_ready = Instant::now();

        tracing::info!(
            "Voice: {}, Kana: {:?}, Speech: {:?}",
            voice_name,
            t_kana_ready - t_start_at,
            t_speech_ready - t_kana_ready,
        );

        let filesize = buffer.len();
        let bodysize = buffer.len() - WAV_HEADER_SIZE;
        let mut file = Cursor::new(buffer);
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(filesize as u32).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00").unwrap();
        file.write_all(&44100u32.to_le_bytes()).unwrap();
        file.write_all(&(44100u32 * 2).to_le_bytes()).unwrap();
        file.write_all(b"\x02\x00\x10\x00data").unwrap();
        file.write_all(&(bodysize as u32).to_le_bytes()).unwrap();

        let buffer = file.into_inner();

        ctx.channel.send(Ok(buffer)).unwrap();
    }
}
