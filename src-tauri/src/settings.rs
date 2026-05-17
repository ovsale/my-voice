use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::Shortcut;

// ============================================================================
// DEFAULT SETTINGS CONSTANTS
// ============================================================================

/// Default modifiers for all hotkeys
pub const DEFAULT_HOTKEY_MODIFIERS: &[&str] = &["ctrl", "alt"];

/// Default key for toggle recording (Ctrl+Alt+Space)
pub const DEFAULT_TOGGLE_KEY: &str = "Space";

/// Default key for hold-to-record (Ctrl+Alt+Backquote)
pub const DEFAULT_HOLD_KEY: &str = "Backquote";

/// Default key for paste last transcription (Ctrl+Alt+.)
pub const DEFAULT_PASTE_LAST_KEY: &str = "Period";

// ============================================================================
// SETTING CLASSIFICATION — all settings are local-only now (no server sync)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalOnlySetting {
    ToggleHotkey,
    HoldHotkey,
    PasteLastHotkey,
    SelectedMicId,
    SoundEnabled,
    VolumeReductionPercent,
    SttProviders,
    ActiveSttProviderIndex,
    SttPrompt,
    PastePrefix,
    OpenaiApiKey,
    LlmFormattingEnabled,
    CleanupPromptSections,
    SendActiveAppContextEnabled,
    OverlaySizePx,
}

impl LocalOnlySetting {
    pub const fn storage_key_name(self) -> &'static str {
        match self {
            Self::ToggleHotkey => "toggle_hotkey",
            Self::HoldHotkey => "hold_hotkey",
            Self::PasteLastHotkey => "paste_last_hotkey",
            Self::SelectedMicId => "selected_mic_id",
            Self::SoundEnabled => "sound_enabled",
            Self::VolumeReductionPercent => "volume_reduction_percent",
            Self::SttProviders => "stt_providers",
            Self::ActiveSttProviderIndex => "active_stt_provider_index",
            Self::SttPrompt => "stt_prompt",
            Self::PastePrefix => "paste_prefix",
            Self::OpenaiApiKey => "openai_api_key",
            Self::LlmFormattingEnabled => "llm_formatting_enabled",
            Self::CleanupPromptSections => "cleanup_prompt_sections",
            Self::SendActiveAppContextEnabled => "send_active_app_context_enabled",
            Self::OverlaySizePx => "overlay_size_px",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingClass {
    LocalOnly(LocalOnlySetting),
}

impl SettingClass {
    pub const fn storage_key_name(self) -> &'static str {
        match self {
            Self::LocalOnly(s) => s.storage_key_name(),
        }
    }
}

impl From<LocalOnlySetting> for SettingClass {
    fn from(value: LocalOnlySetting) -> Self {
        Self::LocalOnly(value)
    }
}

// ============================================================================

/// Enable boolean field by default (needed for serde)
fn default_enabled() -> bool {
    true
}

/// Disable boolean field by default (needed for serde)
fn default_disabled() -> bool {
    false
}

fn default_overlay_size_px() -> u32 {
    48
}

/// Configuration for a hotkey combination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HotkeyConfig {
    /// Modifier keys (e.g., `["ctrl", "alt"]`)
    pub modifiers: Vec<String>,
    /// The main key (e.g., "Space")
    pub key: String,
    /// Whether the hotkey is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self::default_with_key(DEFAULT_TOGGLE_KEY)
    }
}

impl HotkeyConfig {
    fn default_with_key(key: &str) -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            key: key.to_string(),
            enabled: true,
        }
    }

    pub fn default_toggle() -> Self {
        Self::default_with_key(DEFAULT_TOGGLE_KEY)
    }

    pub fn default_hold() -> Self {
        Self::default_with_key(DEFAULT_HOLD_KEY)
    }

    pub fn default_paste_last() -> Self {
        Self::default_with_key(DEFAULT_PASTE_LAST_KEY)
    }

    pub fn to_shortcut_string(&self) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| m.to_lowercase()).collect();
        parts.push(self.key.clone());
        parts.join("+")
    }

    #[cfg(desktop)]
    pub fn to_shortcut(&self) -> Result<Shortcut, String> {
        let shortcut_str = self.to_shortcut_string();
        Shortcut::from_str(&shortcut_str)
            .map_err(|e| format!("Failed to parse shortcut '{shortcut_str}': {e:?}"))
    }

    #[cfg(desktop)]
    pub fn to_shortcut_or_default(&self, default_fn: fn() -> Self) -> Shortcut {
        self.to_shortcut().unwrap_or_else(|_| {
            default_fn()
                .to_shortcut()
                .expect("Default hotkey must be valid")
        })
    }

    pub fn is_same_as(&self, other: &HotkeyConfig) -> bool {
        if self.key.to_lowercase() != other.key.to_lowercase() {
            return false;
        }
        if self.modifiers.len() != other.modifiers.len() {
            return false;
        }
        self.modifiers.iter().all(|mod_a| {
            other
                .modifiers
                .iter()
                .any(|mod_b| mod_a.to_lowercase() == mod_b.to_lowercase())
        })
    }
}

// ============================================================================
// PROMPT SECTION TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode")]
pub enum PromptMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "manual")]
    Manual { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptSection {
    pub enabled: bool,
    #[serde(rename = "mode")]
    pub prompt_mode: PromptMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptSectionType {
    Main,
    Advanced,
    Dictionary,
}

impl PromptSectionType {
    pub const ALL: [Self; 3] = [Self::Main, Self::Advanced, Self::Dictionary];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Advanced => "advanced",
            Self::Dictionary => "dictionary",
        }
    }
}

impl FromStr for PromptSectionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "main" => Ok(Self::Main),
            "advanced" => Ok(Self::Advanced),
            "dictionary" => Ok(Self::Dictionary),
            _ => Err(format!("Unknown prompt section: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CleanupPromptSections {
    pub main: PromptSection,
    pub advanced: PromptSection,
    pub dictionary: PromptSection,
}

impl Default for CleanupPromptSections {
    fn default() -> Self {
        Self {
            main: PromptSection {
                enabled: true,
                prompt_mode: PromptMode::Auto,
            },
            advanced: PromptSection {
                enabled: true,
                prompt_mode: PromptMode::Auto,
            },
            dictionary: PromptSection {
                enabled: true,
                prompt_mode: PromptMode::Auto,
            },
        }
    }
}

impl CleanupPromptSections {
    pub fn get(&self, section_type: PromptSectionType) -> &PromptSection {
        match section_type {
            PromptSectionType::Main => &self.main,
            PromptSectionType::Advanced => &self.advanced,
            PromptSectionType::Dictionary => &self.dictionary,
        }
    }

    pub fn set(&mut self, section_type: PromptSectionType, section: PromptSection) {
        match section_type {
            PromptSectionType::Main => self.main = section,
            PromptSectionType::Advanced => self.advanced = section,
            PromptSectionType::Dictionary => self.dictionary = section,
        }
    }
}

// ============================================================================
// STT PROVIDER
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SttProvider {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    /// `"multipart"` (OpenAI) or `"json"` (OpenRouter)
    #[serde(default = "default_multipart")]
    pub request_format: String,
    #[serde(default)]
    pub extra_body: Option<String>,
}

fn default_multipart() -> String {
    "multipart".to_string()
}

impl Default for SttProvider {
    fn default() -> Self {
        Self {
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1/audio/transcriptions".to_string(),
            model: "whisper-1".to_string(),
            api_key: String::new(),
            request_format: "multipart".to_string(),
            extra_body: None,
        }
    }
}

// ============================================================================
// APP SETTINGS
// ============================================================================

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub toggle_hotkey: HotkeyConfig,
    pub hold_hotkey: HotkeyConfig,
    pub paste_last_hotkey: HotkeyConfig,
    pub selected_mic_id: Option<String>,
    pub sound_enabled: bool,
    #[serde(default)]
    pub cleanup_prompt_sections: Option<CleanupPromptSections>,
    pub volume_reduction_percent: u8,
    #[serde(default)]
    pub stt_providers: Vec<SttProvider>,
    #[serde(default)]
    pub active_stt_provider_index: usize,
    #[serde(default)]
    pub stt_prompt: Option<String>,
    #[serde(default)]
    pub paste_prefix: Option<String>,
    pub openai_api_key: Option<String>,
    #[serde(default = "default_enabled")]
    pub llm_formatting_enabled: bool,
    #[serde(default = "default_disabled")]
    pub send_active_app_context_enabled: bool,
    #[serde(default = "default_overlay_size_px")]
    pub overlay_size_px: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            toggle_hotkey: HotkeyConfig::default_toggle(),
            hold_hotkey: HotkeyConfig::default_hold(),
            paste_last_hotkey: HotkeyConfig::default_paste_last(),
            selected_mic_id: None,
            sound_enabled: true,
            cleanup_prompt_sections: None,
            volume_reduction_percent: 0,
            stt_providers: vec![SttProvider::default()],
            active_stt_provider_index: 0,
            stt_prompt: None,
            paste_prefix: None,
            openai_api_key: None,
            llm_formatting_enabled: true,
            send_active_app_context_enabled: false,
            overlay_size_px: 48,
        }
    }
}

// ============================================================================
// SETTINGS ERRORS
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyType {
    Toggle,
    Hold,
    PasteLast,
}

impl HotkeyType {
    pub fn local_only_setting(self) -> LocalOnlySetting {
        match self {
            Self::Toggle => LocalOnlySetting::ToggleHotkey,
            Self::Hold => LocalOnlySetting::HoldHotkey,
            Self::PasteLast => LocalOnlySetting::PasteLastHotkey,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Hold => "hold",
            Self::PasteLast => "paste last",
        }
    }

    pub fn default_hotkey(self) -> HotkeyConfig {
        match self {
            Self::Toggle => HotkeyConfig::default_toggle(),
            Self::Hold => HotkeyConfig::default_hold(),
            Self::PasteLast => HotkeyConfig::default_paste_last(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SettingsError {
    HotkeyConflict {
        message: String,
        conflicting_type: HotkeyType,
    },
    InvalidValue {
        field: String,
        message: String,
    },
    StoreError(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::HotkeyConflict { message, .. } => write!(f, "{message}"),
            SettingsError::InvalidValue { field, message } => {
                write!(f, "Invalid value for {field}: {message}")
            }
            SettingsError::StoreError(msg) => write!(f, "Store error: {msg}"),
        }
    }
}

impl std::error::Error for SettingsError {}

pub fn check_hotkey_conflict(
    new_hotkey: &HotkeyConfig,
    settings: &AppSettings,
    exclude_type: HotkeyType,
) -> Option<SettingsError> {
    let hotkeys_to_check = [
        (HotkeyType::Toggle, &settings.toggle_hotkey),
        (HotkeyType::Hold, &settings.hold_hotkey),
        (HotkeyType::PasteLast, &settings.paste_last_hotkey),
    ];

    for (hotkey_type, existing) in hotkeys_to_check {
        if hotkey_type != exclude_type && new_hotkey.is_same_as(existing) {
            return Some(SettingsError::HotkeyConflict {
                message: format!(
                    "This shortcut is already used for the {} hotkey",
                    hotkey_type.display_name()
                ),
                conflicting_type: hotkey_type,
            });
        }
    }
    None
}
