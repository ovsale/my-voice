//! Recording orchestrator: mic capture → WAV encode → STT → optional LLM → paste.
//!
//! The mic capture callback (set up in lib.rs) pushes AudioData into the shared
//! buffer. This module provides Tauri commands to start/stop recording and drives
//! the processing pipeline.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};

use crate::audio_encoder;
use crate::events::{EventName, RecordingStatus};
use crate::history::HistoryStorage;
use crate::llm;
use crate::mic_capture::{MicCapture, MicCaptureManager};
use crate::settings::{CleanupPromptSections, LocalOnlySetting, PromptMode};
use crate::stt;

#[cfg(desktop)]
use crate::commands::settings::get_setting_from_store;

/// Shared recording state managed as Tauri state.
///
/// The audio buffer, sample_rate and channels Arcs are cloned and handed to the
/// MicCaptureManager callback in `lib.rs::run()`.
pub struct RecordingOrchestrator {
    status: Arc<Mutex<RecordingStatus>>,
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: Arc<Mutex<u32>>,
    channels: Arc<Mutex<u16>>,
    http_client: reqwest::Client,
}

impl RecordingOrchestrator {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(RecordingStatus::Idle)),
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(44100)),
            channels: Arc::new(Mutex::new(1)),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn audio_buffer(&self) -> Arc<Mutex<Vec<f32>>> {
        self.audio_buffer.clone()
    }

    pub fn sample_rate_holder(&self) -> Arc<Mutex<u32>> {
        self.sample_rate.clone()
    }

    pub fn channels_holder(&self) -> Arc<Mutex<u16>> {
        self.channels.clone()
    }
}

fn emit_status(app: &AppHandle, status: RecordingStatus) {
    let _ = app.emit(EventName::RecordingStatusChanged.as_str(), status);
}

fn set_status(status: &Mutex<RecordingStatus>, new_status: RecordingStatus) {
    if let Ok(mut s) = status.lock() {
        *s = new_status;
    }
}

/// Re-export for use from lib.rs hotkey handler
pub use crate::events::RecordingStatus as RecordingStatusPub;

/// Set recording status from outside (hotkey handler)
pub fn set_status_pub(orch: &RecordingOrchestrator, new_status: RecordingStatusPub) {
    set_status(&orch.status, new_status);
}

/// Trigger the processing pipeline after mic capture has been stopped.
/// Called from the hotkey handler in lib.rs.
pub fn trigger_pipeline(app: AppHandle, orch: &RecordingOrchestrator) {
    set_status(&orch.status, RecordingStatus::Processing);
    emit_status(&app, RecordingStatus::Processing);

    let samples = {
        let mut buf = orch.audio_buffer.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *buf)
    };
    let sample_rate = *orch.sample_rate.lock().unwrap_or_else(|e| e.into_inner());
    let channels = *orch.channels.lock().unwrap_or_else(|e| e.into_inner());

    if samples.is_empty() {
        set_status(&orch.status, RecordingStatus::Idle);
        emit_status(&app, RecordingStatus::Idle);
        log::info!("Recording stopped with no audio data");
        return;
    }

    let status = orch.status.clone();
    let http_client = orch.http_client.clone();

    tauri::async_runtime::spawn(async move {
        let result = run_pipeline(&app, &http_client, samples, sample_rate, channels).await;
        match result {
            Ok(()) => {
                set_status(&status, RecordingStatus::Idle);
                emit_status(&app, RecordingStatus::Idle);
                log::info!("Recording pipeline completed");
            }
            Err(e) => {
                log::error!("Recording pipeline failed: {e:#}");
                set_status(&status, RecordingStatus::Error);
                emit_status(&app, RecordingStatus::Error);
            }
        }
    });
}

// =============================================================================
// Tauri Commands
// =============================================================================

#[tauri::command]
pub async fn start_recording_cmd(
    app: AppHandle,
    recording: tauri::State<'_, RecordingOrchestrator>,
    mic_state: tauri::State<'_, MicCaptureManager>,
    device_id: Option<String>,
) -> Result<(), String> {
    // Clear buffer
    if let Ok(mut buf) = recording.audio_buffer.lock() {
        buf.clear();
    }

    // Start mic capture (the callback in lib.rs pushes into our buffer)
    mic_state
        .capture()
        .start(device_id.as_deref())
        .map_err(|e| e.to_string())?;

    set_status(&recording.status, RecordingStatus::Recording);
    emit_status(&app, RecordingStatus::Recording);

    log::info!("Recording started (device_id: {device_id:?})");
    Ok(())
}

#[tauri::command]
pub async fn stop_recording_cmd(
    app: AppHandle,
    recording: tauri::State<'_, RecordingOrchestrator>,
    mic_state: tauri::State<'_, MicCaptureManager>,
) -> Result<(), String> {
    // Stop mic capture immediately
    mic_state.capture().stop();

    set_status(&recording.status, RecordingStatus::Processing);
    emit_status(&app, RecordingStatus::Processing);

    // Take buffered audio
    let samples = {
        let mut buf = recording.audio_buffer.lock().map_err(|e| e.to_string())?;
        std::mem::take(&mut *buf)
    };
    let sample_rate = *recording.sample_rate.lock().map_err(|e| e.to_string())?;
    let channels = *recording.channels.lock().map_err(|e| e.to_string())?;

    if samples.is_empty() {
        set_status(&recording.status, RecordingStatus::Idle);
        emit_status(&app, RecordingStatus::Idle);
        log::info!("Recording stopped with no audio data");
        return Ok(());
    }

    let status = recording.status.clone();
    let http_client = recording.http_client.clone();

    // Spawn processing pipeline
    tauri::async_runtime::spawn(async move {
        let result =
            run_pipeline(&app, &http_client, samples, sample_rate, channels).await;

        match result {
            Ok(()) => {
                set_status(&status, RecordingStatus::Idle);
                emit_status(&app, RecordingStatus::Idle);
                log::info!("Recording pipeline completed");
            }
            Err(e) => {
                log::error!("Recording pipeline failed: {e:#}");
                set_status(&status, RecordingStatus::Error);
                emit_status(&app, RecordingStatus::Error);
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn get_recording_status(
    recording: tauri::State<'_, RecordingOrchestrator>,
) -> RecordingStatus {
    recording
        .status
        .lock()
        .map(|s| *s)
        .unwrap_or(RecordingStatus::Idle)
}

// =============================================================================
// Processing pipeline
// =============================================================================

async fn run_pipeline(
    app: &AppHandle,
    http_client: &reqwest::Client,
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
) -> anyhow::Result<()> {
    use anyhow::Context;

    log::info!(
        "Processing {} samples ({:.1}s at {}Hz, {}ch)",
        samples.len(),
        samples.len() as f64 / f64::from(sample_rate) / f64::from(channels),
        sample_rate,
        channels
    );

    // 1. Encode to WAV
    let wav_data = audio_encoder::encode_pcm_to_wav(&samples, sample_rate, channels)
        .context("Failed to encode audio to WAV")?;
    log::info!("WAV encoded: {} bytes", wav_data.len());

    // 2. Read API key
    let api_key = read_setting::<Option<String>>(app, LocalOnlySetting::OpenaiApiKey, None)
        .unwrap_or_default();
    if api_key.is_empty() {
        anyhow::bail!("OpenAI API key is not configured. Set it in Settings.");
    }

    // 3. Transcribe via Whisper
    let raw_text = stt::transcribe(http_client, &api_key, wav_data, None)
        .await
        .context("Speech-to-text failed")?;
    log::info!("Transcription: {}", &raw_text[..raw_text.len().min(120)]);

    if raw_text.trim().is_empty() {
        log::info!("Empty transcription, skipping");
        return Ok(());
    }

    // 4. Optionally format via LLM
    let llm_enabled = read_setting(app, LocalOnlySetting::LlmFormattingEnabled, true);
    let final_text = if llm_enabled {
        let system_prompt = build_system_prompt(app);
        match llm::format_text(http_client, &api_key, &system_prompt, &raw_text).await {
            Ok(formatted) => {
                log::info!("LLM formatted: {}", &formatted[..formatted.len().min(120)]);
                formatted
            }
            Err(e) => {
                log::warn!("LLM formatting failed, using raw: {e:#}");
                raw_text.clone()
            }
        }
    } else {
        raw_text.clone()
    };

    // 5. Paste text (must be on main thread for macOS accessibility)
    let text_to_paste = final_text.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        let result = crate::commands::text::type_text_blocking(&text_to_paste);
        let _ = tx.send(result);
    })
    .context("Failed to dispatch paste to main thread")?;

    rx.recv()
        .context("Failed to receive paste result")?
        .map_err(|e| anyhow::anyhow!("Paste failed: {e}"))?;

    // 6. Add to history
    if let Some(history) = app.try_state::<HistoryStorage>() {
        if let Err(e) = history.add_entry(final_text, raw_text, None) {
            log::warn!("Failed to save to history: {e:#}");
        } else {
            let _ = app.emit(EventName::HistoryChanged.as_str(), ());
        }
    }

    Ok(())
}

// =============================================================================
// Settings helpers
// =============================================================================

#[cfg(desktop)]
fn read_setting<T: serde::de::DeserializeOwned>(
    app: &AppHandle,
    setting: LocalOnlySetting,
    default: T,
) -> T {
    get_setting_from_store(app, setting, default)
}

#[cfg(not(desktop))]
fn read_setting<T>(_app: &AppHandle, _setting: LocalOnlySetting, default: T) -> T {
    default
}

fn build_system_prompt(app: &AppHandle) -> String {
    let sections: Option<CleanupPromptSections> =
        read_setting(app, LocalOnlySetting::CleanupPromptSections, None);

    if let Some(sections) = sections {
        let mut parts: Vec<String> = Vec::new();

        if sections.main.enabled {
            match &sections.main.prompt_mode {
                PromptMode::Manual { content } => parts.push(content.clone()),
                PromptMode::Auto => parts.push(default_system_prompt()),
            }
        }

        if sections.advanced.enabled {
            if let PromptMode::Manual { content } = &sections.advanced.prompt_mode {
                parts.push(content.clone());
            }
        }

        if sections.dictionary.enabled {
            if let PromptMode::Manual { content } = &sections.dictionary.prompt_mode {
                if !content.is_empty() {
                    parts.push(format!("Dictionary/terminology: {content}"));
                }
            }
        }

        if parts.is_empty() {
            return default_system_prompt();
        }

        parts.join("\n\n")
    } else {
        default_system_prompt()
    }
}

fn default_system_prompt() -> String {
    "Clean up and format the following transcribed speech. \
     Fix grammar, punctuation, and capitalization. \
     Remove filler words like 'um', 'uh', 'like', 'you know'. \
     Keep the original meaning and tone. \
     Output only the cleaned text with no extra commentary."
        .to_string()
}
