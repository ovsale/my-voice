use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

use crate::history::{HistoryEntry, HistoryImportResult, HistoryImportStrategy, HistoryStorage};
use crate::settings::{
    AppSettings, CleanupPromptSections, LocalOnlySetting, PromptMode, PromptSection,
    PromptSectionType, SettingClass,
};

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

// ============================================================================
// EXPORT FILE STRUCTURES
// ============================================================================

const EXPORT_VERSION: u32 = 1;
const SETTINGS_EXPORT_TYPE: &str = "my-voice-settings";
const HISTORY_EXPORT_TYPE: &str = "my-voice-history";
const PROMPT_COMMENT_PREFIX: &str = "<!-- my-voice-prompt: ";
const PROMPT_COMMENT_SUFFIX: &str = " -->";

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsExportData {
    pub toggle_hotkey: crate::settings::HotkeyConfig,
    pub hold_hotkey: crate::settings::HotkeyConfig,
    pub paste_last_hotkey: crate::settings::HotkeyConfig,
    pub selected_mic_id: Option<String>,
    pub sound_enabled: bool,
    pub auto_mute_audio: bool,
    pub openai_api_key: Option<String>,
    pub llm_formatting_enabled: bool,
    pub send_active_app_context_enabled: bool,
}

impl Default for SettingsExportData {
    fn default() -> Self {
        AppSettings::default().into()
    }
}

impl From<AppSettings> for SettingsExportData {
    fn from(s: AppSettings) -> Self {
        Self {
            toggle_hotkey: s.toggle_hotkey,
            hold_hotkey: s.hold_hotkey,
            paste_last_hotkey: s.paste_last_hotkey,
            selected_mic_id: s.selected_mic_id,
            sound_enabled: s.sound_enabled,
            auto_mute_audio: s.auto_mute_audio,
            openai_api_key: s.openai_api_key,
            llm_formatting_enabled: s.llm_formatting_enabled,
            send_active_app_context_enabled: s.send_active_app_context_enabled,
        }
    }
}

impl From<SettingsExportData> for AppSettings {
    fn from(e: SettingsExportData) -> Self {
        Self {
            toggle_hotkey: e.toggle_hotkey,
            hold_hotkey: e.hold_hotkey,
            paste_last_hotkey: e.paste_last_hotkey,
            selected_mic_id: e.selected_mic_id,
            sound_enabled: e.sound_enabled,
            cleanup_prompt_sections: None,
            auto_mute_audio: e.auto_mute_audio,
            openai_api_key: e.openai_api_key,
            llm_formatting_enabled: e.llm_formatting_enabled,
            send_active_app_context_enabled: e.send_active_app_context_enabled,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsExportFile {
    #[serde(rename = "type")]
    pub file_type: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub data: SettingsExportData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryExportFile {
    #[serde(rename = "type")]
    pub file_type: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub entry_count: usize,
    pub data: Vec<HistoryEntry>,
}

// ============================================================================
// IMPORT RESULT TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedFileType {
    Settings,
    History,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeApplyWarningCode {
    #[serde(rename = "focus_watcher_reconcile_failed")]
    FocusWatcherReconcile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeApplyAction {
    FocusWatcherEnabled,
    FocusWatcherDisabled,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeApplyWarning {
    pub code: RuntimeApplyWarningCode,
    pub message: String,
    #[serde(serialize_with = "serialize_setting_class_as_key")]
    pub setting_key: SettingClass,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeActionApplied {
    pub action: RuntimeApplyAction,
    #[serde(serialize_with = "serialize_setting_class_as_key")]
    pub setting_key: SettingClass,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RuntimeApplyOutcome {
    pub warnings: Vec<RuntimeApplyWarning>,
    pub runtime_actions_applied: Vec<RuntimeActionApplied>,
}

pub type ImportSettingsOutcome = RuntimeApplyOutcome;
pub type FactoryResetOutcome = RuntimeApplyOutcome;

#[derive(Debug, Deserialize)]
struct FileTypeProbe {
    #[serde(rename = "type")]
    file_type: Option<String>,
    version: Option<u32>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_setting_class_as_key<S>(sc: &SettingClass, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(sc.storage_key_name())
}

// ============================================================================
// COMMANDS
// ============================================================================

#[cfg(desktop)]
#[tauri::command]
pub fn generate_settings_export(app: AppHandle) -> Result<String, String> {
    let settings = super::settings::get_settings(app)?;
    let export = SettingsExportFile {
        file_type: SETTINGS_EXPORT_TYPE.to_string(),
        version: EXPORT_VERSION,
        exported_at: Utc::now(),
        data: settings.into(),
    };
    serde_json::to_string_pretty(&export).map_err(|e| format!("Failed to serialize: {e}"))
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn generate_settings_export(_app: AppHandle) -> Result<String, String> {
    Err("Not supported".to_string())
}

#[tauri::command]
pub fn generate_history_export(app: AppHandle) -> Result<String, String> {
    let history = app.state::<HistoryStorage>();
    let entries = history.get_all(None).map_err(|e| format!("{e}"))?;
    let export = HistoryExportFile {
        file_type: HISTORY_EXPORT_TYPE.to_string(),
        version: EXPORT_VERSION,
        exported_at: Utc::now(),
        entry_count: entries.len(),
        data: entries,
    };
    serde_json::to_string_pretty(&export).map_err(|e| format!("{e}"))
}

#[cfg(desktop)]
#[tauri::command]
pub fn generate_prompt_exports(
    app: AppHandle,
) -> Result<HashMap<PromptSectionType, String>, String> {
    let settings = super::settings::get_settings(app)?;
    let mut prompts = HashMap::new();
    if let Some(sections) = settings.cleanup_prompt_sections {
        for section_type in PromptSectionType::ALL {
            let section = sections.get(section_type);
            let mode_str = match &section.prompt_mode {
                PromptMode::Auto => "auto",
                PromptMode::Manual { .. } => "manual",
            };
            let content = match &section.prompt_mode {
                PromptMode::Auto => String::new(),
                PromptMode::Manual { content } => content.clone(),
            };
            prompts.insert(
                section_type,
                format!(
                    "{}{}{}\nenabled: {}\nmode: {}\n---\n{}",
                    PROMPT_COMMENT_PREFIX,
                    section_type.as_str(),
                    PROMPT_COMMENT_SUFFIX,
                    section.enabled,
                    mode_str,
                    content
                ),
            );
        }
    }
    Ok(prompts)
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn generate_prompt_exports(
    _app: AppHandle,
) -> Result<HashMap<PromptSectionType, String>, String> {
    Ok(HashMap::new())
}

#[tauri::command]
pub fn parse_prompt_file(content: String) -> Result<(PromptSectionType, String), String> {
    let trimmed = content.trim();
    if !trimmed.starts_with(PROMPT_COMMENT_PREFIX) {
        return Err("Not a valid prompt file: missing header".to_string());
    }
    let after_prefix = &trimmed[PROMPT_COMMENT_PREFIX.len()..];
    let suffix_pos = after_prefix
        .find(PROMPT_COMMENT_SUFFIX)
        .ok_or("Malformed header")?;
    let section_name = after_prefix[..suffix_pos].trim();
    let section_type = section_name.parse::<PromptSectionType>()?;
    let content_start = PROMPT_COMMENT_PREFIX.len() + suffix_pos + PROMPT_COMMENT_SUFFIX.len();
    let prompt_content = trimmed[content_start..].trim().to_string();
    Ok((section_type, prompt_content))
}

#[cfg(desktop)]
#[tauri::command]
pub async fn import_prompt(
    app: AppHandle,
    section: PromptSectionType,
    content: String,
) -> Result<(), String> {
    use super::settings::get_setting_from_store;

    let mut sections: CleanupPromptSections = get_setting_from_store(
        &app,
        LocalOnlySetting::CleanupPromptSections,
        CleanupPromptSections::default(),
    );

    let lines: Vec<&str> = content.lines().collect();
    let enabled = lines
        .iter()
        .find(|line| line.starts_with("enabled:"))
        .and_then(|line| line.strip_prefix("enabled:"))
        .is_none_or(|s| s.trim() == "true");
    let mode = lines
        .iter()
        .find(|line| line.starts_with("mode:"))
        .and_then(|line| line.strip_prefix("mode:"))
        .map_or("auto", str::trim);
    let content_start = lines.iter().position(|line| line.trim() == "---");
    let prompt_content = if let Some(idx) = content_start {
        lines[idx + 1..].join("\n")
    } else {
        content.clone()
    };
    let prompt_mode = if mode == "manual" {
        PromptMode::Manual {
            content: prompt_content,
        }
    } else {
        PromptMode::Auto
    };
    sections.set(section, PromptSection { enabled, prompt_mode });
    crate::save_setting_to_store(&app, LocalOnlySetting::CleanupPromptSections.into(), &sections)
        .map_err(|e| format!("{e:#}"))?;
    log::info!("Imported prompt for section: {}", section.as_str());
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn import_prompt(
    _app: AppHandle,
    _section: PromptSectionType,
    _content: String,
) -> Result<(), String> {
    Err("Not supported".to_string())
}

#[tauri::command]
pub fn detect_export_file_type(content: String) -> DetectedFileType {
    match serde_json::from_str::<FileTypeProbe>(&content) {
        Ok(probe) => match probe.file_type.as_deref() {
            Some(SETTINGS_EXPORT_TYPE) if probe.version.is_some_and(|v| v <= EXPORT_VERSION) => {
                DetectedFileType::Settings
            }
            Some(HISTORY_EXPORT_TYPE) if probe.version.is_some_and(|v| v <= EXPORT_VERSION) => {
                DetectedFileType::History
            }
            // Also accept old Tambourine format for backwards compat
            Some("tambourine-settings")
                if probe.version.is_some_and(|v| v <= EXPORT_VERSION) =>
            {
                DetectedFileType::Settings
            }
            Some("tambourine-history")
                if probe.version.is_some_and(|v| v <= EXPORT_VERSION) =>
            {
                DetectedFileType::History
            }
            _ => DetectedFileType::Unknown,
        },
        Err(_) => DetectedFileType::Unknown,
    }
}

// ============================================================================
// SETTING CLASSES FOR IMPORT/EXPORT
// ============================================================================

const IMPORT_EXPORT_SETTING_CLASSES: [SettingClass; 9] = [
    SettingClass::LocalOnly(LocalOnlySetting::ToggleHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::HoldHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::PasteLastHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::SelectedMicId),
    SettingClass::LocalOnly(LocalOnlySetting::SoundEnabled),
    SettingClass::LocalOnly(LocalOnlySetting::AutoMuteAudio),
    SettingClass::LocalOnly(LocalOnlySetting::OpenaiApiKey),
    SettingClass::LocalOnly(LocalOnlySetting::LlmFormattingEnabled),
    SettingClass::LocalOnly(LocalOnlySetting::SendActiveAppContextEnabled),
];

fn serialized_value_for_setting(
    settings: &AppSettings,
    setting: SettingClass,
) -> anyhow::Result<serde_json::Value> {
    let SettingClass::LocalOnly(local) = setting;
    let val = match local {
        LocalOnlySetting::ToggleHotkey => serde_json::to_value(&settings.toggle_hotkey),
        LocalOnlySetting::HoldHotkey => serde_json::to_value(&settings.hold_hotkey),
        LocalOnlySetting::PasteLastHotkey => serde_json::to_value(&settings.paste_last_hotkey),
        LocalOnlySetting::SelectedMicId => serde_json::to_value(&settings.selected_mic_id),
        LocalOnlySetting::SoundEnabled => serde_json::to_value(settings.sound_enabled),
        LocalOnlySetting::AutoMuteAudio => serde_json::to_value(settings.auto_mute_audio),
        LocalOnlySetting::OpenaiApiKey => serde_json::to_value(&settings.openai_api_key),
        LocalOnlySetting::LlmFormattingEnabled => {
            serde_json::to_value(settings.llm_formatting_enabled)
        }
        LocalOnlySetting::CleanupPromptSections => {
            serde_json::to_value(&settings.cleanup_prompt_sections)
        }
        LocalOnlySetting::SendActiveAppContextEnabled => {
            serde_json::to_value(settings.send_active_app_context_enabled)
        }
    };
    val.with_context(|| format!("Failed to serialize '{}'", setting.storage_key_name()))
}

#[cfg(desktop)]
fn apply_runtime_side_effects(
    app: &AppHandle,
    send_active_app_context_enabled: bool,
) -> RuntimeApplyOutcome {
    let mut outcome = RuntimeApplyOutcome::default();
    match super::settings::reconcile_focus_watcher_enabled_state(app, send_active_app_context_enabled)
    {
        Ok(()) => {
            let action = if send_active_app_context_enabled {
                RuntimeApplyAction::FocusWatcherEnabled
            } else {
                RuntimeApplyAction::FocusWatcherDisabled
            };
            outcome.runtime_actions_applied.push(RuntimeActionApplied {
                action,
                setting_key: LocalOnlySetting::SendActiveAppContextEnabled.into(),
            });
        }
        Err(error) => {
            outcome.warnings.push(RuntimeApplyWarning {
                code: RuntimeApplyWarningCode::FocusWatcherReconcile,
                setting_key: LocalOnlySetting::SendActiveAppContextEnabled.into(),
                message: format!("Failed to sync focus watcher: {error:#}"),
            });
        }
    }
    outcome
}

#[cfg(desktop)]
#[tauri::command]
pub async fn import_settings(
    app: AppHandle,
    content: String,
) -> Result<ImportSettingsOutcome, String> {
    let export: SettingsExportFile =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse: {e}"))?;

    if export.file_type != SETTINGS_EXPORT_TYPE && export.file_type != "tambourine-settings" {
        return Err(format!("Invalid file type: '{}'", export.file_type));
    }
    if export.version > EXPORT_VERSION {
        return Err(format!("Unsupported version: {}", export.version));
    }

    let store = app.store("settings.json").map_err(|e| format!("{e}"))?;
    let imported: AppSettings = export.data.into();

    for &sc in &IMPORT_EXPORT_SETTING_CLASSES {
        let val = serialized_value_for_setting(&imported, sc).map_err(|e| format!("{e:#}"))?;
        store.set(sc.storage_key_name(), val);
    }
    store.save().map_err(|e| format!("{e}"))?;

    let outcome = apply_runtime_side_effects(&app, imported.send_active_app_context_enabled);
    log::info!("Settings imported successfully");
    Ok(outcome)
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn import_settings(
    _app: AppHandle,
    _content: String,
) -> Result<ImportSettingsOutcome, String> {
    Err("Not supported".to_string())
}

#[tauri::command]
pub fn import_history(
    app: AppHandle,
    content: String,
    strategy: HistoryImportStrategy,
) -> Result<HistoryImportResult, String> {
    let export: HistoryExportFile =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse: {e}"))?;

    if export.file_type != HISTORY_EXPORT_TYPE && export.file_type != "tambourine-history" {
        return Err(format!("Invalid file type: '{}'", export.file_type));
    }
    if export.version > EXPORT_VERSION {
        return Err(format!("Unsupported version: {}", export.version));
    }

    let history = app.state::<HistoryStorage>();
    history
        .import_entries(export.data, strategy)
        .map_err(|e| e.to_string())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn factory_reset(app: AppHandle) -> Result<FactoryResetOutcome, String> {
    let store = app.store("settings.json").map_err(|e| format!("{e}"))?;
    store.clear();
    store.save().map_err(|e| format!("{e}"))?;

    let history = app.state::<HistoryStorage>();
    history.clear().map_err(|e| e.to_string())?;

    let defaults = AppSettings::default();
    for &sc in &IMPORT_EXPORT_SETTING_CLASSES {
        let val = serialized_value_for_setting(&defaults, sc).map_err(|e| format!("{e:#}"))?;
        store.set(sc.storage_key_name(), val);
    }
    store.save().map_err(|e| format!("{e}"))?;

    let outcome = apply_runtime_side_effects(&app, defaults.send_active_app_context_enabled);
    log::info!("Factory reset completed");
    Ok(outcome)
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn factory_reset(_app: AppHandle) -> Result<FactoryResetOutcome, String> {
    Err("Not supported".to_string())
}
