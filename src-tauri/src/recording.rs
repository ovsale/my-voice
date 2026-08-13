//! Recording orchestrator: mic capture → WAV encode → STT → optional LLM → paste.
//!
//! The mic capture callback (set up in lib.rs) pushes AudioData into the shared
//! buffer. This module provides Tauri commands to start/stop recording and drives
//! the processing pipeline.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use tauri::async_runtime::JoinHandle;

use crate::audio_encoder;
use crate::events::{EventName, RecordingStatus};
use crate::history::HistoryStorage;
use crate::llm;
use crate::mic_capture::{MicCapture, MicCaptureManager};
use crate::settings::{CleanupPromptSections, LocalOnlySetting, PromptMode, SttProvider};
use crate::state::{AppState, ShortcutState};
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
    /// Handle to the running pipeline task, used for cancellation.
    pipeline_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl RecordingOrchestrator {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(RecordingStatus::Idle)),
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(44100)),
            channels: Arc::new(Mutex::new(1)),
            http_client: reqwest::Client::new(),
            pipeline_handle: Arc::new(Mutex::new(None)),
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

/// Cancel the currently running pipeline (if any).
/// Full cancel: the in-flight history entry and the saved clip are dropped.
/// Returns true if a pipeline was actually cancelled.
pub fn cancel_pipeline(app: &AppHandle, orch: &RecordingOrchestrator) -> bool {
    let handle = {
        let mut guard = orch.pipeline_handle.lock().unwrap_or_else(|e| e.into_inner());
        guard.take()
    };
    if let Some(h) = handle {
        h.abort();

        if let Some(entry_id) = get_last_recording_entry_id(app.clone()) {
            if let Some(history) = app.try_state::<HistoryStorage>() {
                match history.delete_entry_if_processing(&entry_id) {
                    Ok(true) => {
                        let _ = app.emit(EventName::HistoryChanged.as_str(), ());
                    }
                    Ok(false) => {}
                    Err(e) => log::warn!("Failed to delete cancelled entry: {e:#}"),
                }
            }
        }
        clear_last_recording(app);

        set_status(&orch.status, RecordingStatus::Idle);
        emit_status(app, RecordingStatus::Idle);
        log::info!("Pipeline cancelled by user");
        true
    } else {
        false
    }
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
    let pipeline_handle_ref = orch.pipeline_handle.clone();

    let handle = tauri::async_runtime::spawn(async move {
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
        // Reset shortcut state machine back to Idle
        if let Some(app_state) = app.try_state::<AppState>() {
            if let Ok(mut s) = app_state.shortcut_state.lock() {
                *s = ShortcutState::Idle;
            }
        }
        // Clear the handle once done
        if let Ok(mut guard) = pipeline_handle_ref.lock() {
            *guard = None;
        }
    });

    // Store handle for possible cancellation
    if let Ok(mut guard) = orch.pipeline_handle.lock() {
        *guard = Some(handle);
    }
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
    let pipeline_handle_ref = recording.pipeline_handle.clone();

    // Spawn processing pipeline
    let handle = tauri::async_runtime::spawn(async move {
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
        if let Ok(mut guard) = pipeline_handle_ref.lock() {
            *guard = None;
        }
    });

    if let Ok(mut guard) = recording.pipeline_handle.lock() {
        *guard = Some(handle);
    }

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

    // 2. Persist the clip before any network call so a failed transcription
    //    can be retried later (possibly with different settings/provider)
    save_last_recording_wav(app, &wav_data);

    // 3. The entry appears in history immediately with a Processing status
    let entry_id = if let Some(history) = app.try_state::<HistoryStorage>() {
        match history.add_processing_entry() {
            Ok(entry) => {
                save_last_recording_meta(app, &entry.id);
                let _ = app.emit(EventName::HistoryChanged.as_str(), ());
                Some(entry.id)
            }
            Err(e) => {
                log::warn!("Failed to add processing entry to history: {e:#}");
                None
            }
        }
    } else {
        None
    };

    // 4. Transcribe + format (STT → LLM → prefix)
    let (final_text, raw_text) = match transcribe_and_format(app, http_client, wav_data).await {
        Ok(Some(texts)) => texts,
        Ok(None) => {
            // Empty result also counts as a failed attempt: the clip stays
            // reachable so the user can re-transcribe with another provider
            log::info!("Empty transcription");
            finish_entry_failed(
                app,
                entry_id.as_deref(),
                "Transcription returned empty text".to_string(),
            );
            return Ok(());
        }
        Err(e) => {
            finish_entry_failed(app, entry_id.as_deref(), format!("{e:#}"));
            return Err(e);
        }
    };

    // 5. Fill the entry before pasting so the text survives a paste failure
    if let Some(history) = app.try_state::<HistoryStorage>() {
        match &entry_id {
            Some(id) => {
                match history.update_entry_transcription(id, final_text.clone(), raw_text) {
                    Ok(true) => {}
                    Ok(false) => log::info!("History entry {id} was deleted before completion"),
                    Err(e) => log::warn!("Failed to update history entry: {e:#}"),
                }
            }
            None => {
                // Processing entry could not be created earlier; save the result anyway
                match history.add_entry(final_text.clone(), raw_text, None) {
                    Ok(entry) => save_last_recording_meta(app, &entry.id),
                    Err(e) => log::warn!("Failed to save to history: {e:#}"),
                }
            }
        }
        let _ = app.emit(EventName::HistoryChanged.as_str(), ());
    }

    // 6. Paste text
    let text_to_paste = final_text;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        let result = crate::commands::text::type_text_blocking(&text_to_paste);
        let _ = tx.send(result);
    })
    .context("Failed to dispatch paste to main thread")?;

    rx.recv()
        .context("Failed to receive paste result")?
        .map_err(|e| anyhow::anyhow!("Paste failed: {e}"))?;

    Ok(())
}

/// Mark the in-flight history entry as failed and refresh the feed
fn finish_entry_failed(app: &AppHandle, entry_id: Option<&str>, message: String) {
    let Some(id) = entry_id else {
        log::warn!("Transcription failed with no history entry to record it: {message}");
        return;
    };
    if let Some(history) = app.try_state::<HistoryStorage>() {
        if let Err(e) = history.mark_entry_failed(id, message) {
            log::warn!("Failed to mark history entry as failed: {e:#}");
        }
        let _ = app.emit(EventName::HistoryChanged.as_str(), ());
    }
}

/// STT → trim → optional LLM formatting → optional prefix.
/// Reads all settings fresh, so a provider/model switch applies to re-transcribes.
/// Returns `Ok(None)` when the transcription came back empty.
async fn transcribe_and_format(
    app: &AppHandle,
    http_client: &reqwest::Client,
    wav_data: Vec<u8>,
) -> anyhow::Result<Option<(String, String)>> {
    use anyhow::Context;

    // Read active STT provider
    let providers: Vec<SttProvider> = read_setting(
        app,
        LocalOnlySetting::SttProviders,
        vec![SttProvider::default()],
    );
    let active_index: usize = read_setting(app, LocalOnlySetting::ActiveSttProviderIndex, 0);
    let provider = providers
        .get(active_index)
        .or_else(|| providers.first())
        .context("No STT providers configured")?;

    if provider.api_key.is_empty() {
        anyhow::bail!(
            "STT API key is not configured for provider '{}'. Set it in Settings.",
            provider.name
        );
    }

    let format = stt::SttRequestFormat::from_str_lossy(&provider.request_format);
    log::info!(
        "Using STT provider '{}' (format: {}, model: {})",
        provider.name,
        format.as_str(),
        provider.model
    );

    // Transcribe via STT API
    let stt_prompt: Option<String> = read_setting(app, LocalOnlySetting::SttPrompt, None);
    let stt_config = stt::SttConfig {
        base_url: &provider.base_url,
        model: &provider.model,
        api_key: &provider.api_key,
        format,
        prompt: stt_prompt.as_deref(),
        extra_body: provider.extra_body.as_deref(),
    };
    let raw_text = stt::transcribe(http_client, &stt_config, wav_data, None)
        .await
        .context("Speech-to-text failed")?
        .trim()
        .to_string();
    log::info!("Transcription: {:.120}", raw_text);

    if raw_text.is_empty() {
        return Ok(None);
    }

    // Optionally format via LLM
    let llm_enabled = read_setting(app, LocalOnlySetting::LlmFormattingEnabled, true);
    let final_text = if llm_enabled {
        let llm_api_key =
            read_setting::<Option<String>>(app, LocalOnlySetting::OpenaiApiKey, None)
                .unwrap_or_default();
        let system_prompt = build_system_prompt(app);
        match llm::format_text(http_client, &llm_api_key, &system_prompt, &raw_text).await {
            Ok(formatted) => {
                log::info!("LLM formatted: {:.120}", formatted);
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

    // Apply optional prefix
    let paste_prefix: Option<String> = read_setting(app, LocalOnlySetting::PastePrefix, None);
    let final_text = if let Some(prefix) = &paste_prefix {
        if prefix.is_empty() {
            final_text
        } else {
            format!("{prefix}{final_text}")
        }
    } else {
        final_text
    };

    Ok(Some((final_text, raw_text)))
}

// =============================================================================
// Last recording persistence (single slot: the most recent clip)
// =============================================================================

const LAST_RECORDING_WAV: &str = "last_recording.wav";
const LAST_RECORDING_META: &str = "last_recording.json";

/// Links the saved WAV to the history entry it belongs to
#[derive(serde::Serialize, serde::Deserialize)]
struct LastRecordingMeta {
    entry_id: String,
}

fn last_recording_paths(app: &AppHandle) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = app.path().app_data_dir().ok()?;
    Some((dir.join(LAST_RECORDING_WAV), dir.join(LAST_RECORDING_META)))
}

/// Overwrite the last-recording slot with a new clip.
/// Removes the meta first so a stale link never points at a mismatched clip.
fn save_last_recording_wav(app: &AppHandle, wav_data: &[u8]) {
    let Some((wav_path, meta_path)) = last_recording_paths(app) else {
        log::warn!("Cannot resolve app data dir; last recording will not be saved");
        return;
    };
    let _ = std::fs::remove_file(&meta_path);
    if let Err(e) = std::fs::write(&wav_path, wav_data) {
        log::warn!("Failed to save last recording WAV: {e}");
        let _ = std::fs::remove_file(&wav_path);
    }
}

/// Link the saved WAV to a history entry
fn save_last_recording_meta(app: &AppHandle, entry_id: &str) {
    let Some((_, meta_path)) = last_recording_paths(app) else {
        return;
    };
    let meta = LastRecordingMeta {
        entry_id: entry_id.to_string(),
    };
    match serde_json::to_string(&meta) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&meta_path, json) {
                log::warn!("Failed to save last recording meta: {e}");
            }
        }
        Err(e) => log::warn!("Failed to serialize last recording meta: {e}"),
    }
}

/// Remove the saved clip and its link entirely (full cancel)
fn clear_last_recording(app: &AppHandle) {
    if let Some((wav_path, meta_path)) = last_recording_paths(app) {
        let _ = std::fs::remove_file(meta_path);
        let _ = std::fs::remove_file(wav_path);
    }
}

/// Load the saved clip and the ID of the history entry it belongs to
fn load_last_recording(app: &AppHandle) -> Option<(String, Vec<u8>)> {
    let (wav_path, meta_path) = last_recording_paths(app)?;
    let meta_content = std::fs::read_to_string(meta_path).ok()?;
    let meta: LastRecordingMeta = serde_json::from_str(&meta_content).ok()?;
    let wav_data = std::fs::read(wav_path).ok()?;
    Some((meta.entry_id, wav_data))
}

/// ID of the history entry that owns the saved last recording, if any.
/// The frontend uses this to show the "Re-transcribe" action on that entry.
#[tauri::command]
pub fn get_last_recording_entry_id(app: AppHandle) -> Option<String> {
    let (wav_path, meta_path) = last_recording_paths(&app)?;
    if !wav_path.exists() {
        return None;
    }
    let meta_content = std::fs::read_to_string(meta_path).ok()?;
    serde_json::from_str::<LastRecordingMeta>(&meta_content)
        .ok()
        .map(|meta| meta.entry_id)
}

/// Re-run STT → LLM → prefix on the saved last recording with current settings.
/// Updates the owning history entry in place; never pastes (the app window is
/// focused when this runs, Cmd+V would paste into My Voice itself).
#[tauri::command]
pub async fn retranscribe_last(
    app: AppHandle,
    recording: tauri::State<'_, RecordingOrchestrator>,
) -> Result<(), String> {
    let current_status = recording
        .status
        .lock()
        .map(|s| *s)
        .unwrap_or(RecordingStatus::Idle);
    if matches!(
        current_status,
        RecordingStatus::Recording | RecordingStatus::Processing
    ) {
        return Err("Recording or processing is already in progress".to_string());
    }

    let (entry_id, wav_data) = load_last_recording(&app)
        .ok_or_else(|| "No saved recording available to re-transcribe".to_string())?;

    let history = app
        .try_state::<HistoryStorage>()
        .ok_or_else(|| "History storage is unavailable".to_string())?;

    // Show progress: the entry switches to Processing, the overlay reuses the
    // regular pipeline status events
    match history.set_entry_processing(&entry_id) {
        Ok(true) => {
            let _ = app.emit(EventName::HistoryChanged.as_str(), ());
        }
        Ok(false) => {}
        Err(e) => log::warn!("Failed to set entry processing: {e:#}"),
    }
    set_status(&recording.status, RecordingStatus::Processing);
    emit_status(&app, RecordingStatus::Processing);

    let http_client = recording.http_client.clone();
    let result = transcribe_and_format(&app, &http_client, wav_data).await;

    // Restore the overlay unless a new recording started meanwhile
    {
        let mut status = recording
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *status == RecordingStatus::Processing {
            *status = RecordingStatus::Idle;
            emit_status(&app, RecordingStatus::Idle);
        }
    }

    match result {
        Ok(Some((final_text, raw_text))) => {
            let updated = history
                .update_entry_transcription(&entry_id, final_text.clone(), raw_text.clone())
                .map_err(|e| e.to_string())?;
            if !updated {
                // The entry was deleted meanwhile; re-link the clip to a fresh
                // entry, but only if a newer recording hasn't claimed the slot
                let entry = history
                    .add_entry(final_text, raw_text, None)
                    .map_err(|e| e.to_string())?;
                let slot_still_ours = get_last_recording_entry_id(app.clone())
                    .is_some_and(|current_id| current_id == entry_id);
                if slot_still_ours {
                    save_last_recording_meta(&app, &entry.id);
                }
            }
            let _ = app.emit(EventName::HistoryChanged.as_str(), ());
            log::info!("Re-transcription succeeded for entry {entry_id}");
            Ok(())
        }
        Ok(None) => {
            let message = "Transcription returned empty text".to_string();
            if let Err(history_error) = history.mark_entry_failed(&entry_id, message.clone()) {
                log::warn!("Failed to mark history entry as failed: {history_error:#}");
            }
            let _ = app.emit(EventName::HistoryChanged.as_str(), ());
            Err(message)
        }
        Err(e) => {
            let message = format!("{e:#}");
            log::error!("Re-transcription failed for entry {entry_id}: {message}");
            if let Err(history_error) = history.mark_entry_failed(&entry_id, message.clone()) {
                log::warn!("Failed to mark history entry as failed: {history_error:#}");
            }
            let _ = app.emit(EventName::HistoryChanged.as_str(), ());
            Err(message)
        }
    }
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
