use crate::settings::{
    check_hotkey_conflict, AppSettings, CleanupPromptSections, HotkeyConfig, HotkeyType,
    LocalOnlySetting, SettingClass, SettingsError,
};
use crate::state::{AppState, ShortcutErrors, ShortcutRegistrationResult};
use anyhow::{anyhow, Context};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(desktop)]
use crate::active_app_context::sync_focus_watcher_enabled;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

#[cfg(desktop)]
#[tauri::command]
pub async fn unregister_shortcuts(app: AppHandle) -> Result<(), String> {
    log::info!("Temporarily unregistering all shortcuts for hotkey capture");
    let shortcut_manager = app.global_shortcut();
    shortcut_manager
        .unregister_all()
        .map_err(|e| format!("Failed to unregister shortcuts: {e}"))?;
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn unregister_shortcuts(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
pub(crate) fn get_setting_from_store<T: serde::de::DeserializeOwned>(
    app: &AppHandle,
    setting_class: impl Into<SettingClass>,
    default: T,
) -> T {
    let storage_key_name = setting_class.into().storage_key_name();
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get(storage_key_name))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(default)
}

fn persist_setting<T: serde::Serialize>(
    app: &AppHandle,
    setting: LocalOnlySetting,
    value: &T,
) -> anyhow::Result<()> {
    crate::save_setting_to_store(app, setting.into(), value).with_context(|| {
        format!(
            "Failed to persist setting '{}'",
            setting.storage_key_name()
        )
    })?;
    let _ = app.emit(crate::events::EventName::SettingsChanged.as_str(), ());
    Ok(())
}

#[cfg(desktop)]
pub(crate) fn reconcile_focus_watcher_enabled_state(
    app: &AppHandle,
    send_active_app_context_enabled: bool,
) -> anyhow::Result<()> {
    let app_state = app
        .try_state::<AppState>()
        .context("AppState unavailable while reconciling focus watcher lifecycle")?;

    let mut focus_watcher_guard = app_state.focus_watcher.lock().map_err(|lock_error| {
        anyhow!("Failed to lock focus watcher state for reconciliation: {lock_error}")
    })?;

    sync_focus_watcher_enabled(
        app,
        &mut focus_watcher_guard,
        send_active_app_context_enabled,
    );
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn register_shortcuts(app: AppHandle) -> Result<ShortcutRegistrationResult, String> {
    Ok(crate::do_register_shortcuts(&app))
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn register_shortcuts(_app: AppHandle) -> Result<ShortcutRegistrationResult, String> {
    Ok(ShortcutRegistrationResult {
        toggle_registered: true,
        hold_registered: true,
        paste_last_registered: true,
        errors: ShortcutErrors::default(),
    })
}

#[tauri::command]
pub fn get_shortcut_errors(app: AppHandle) -> ShortcutErrors {
    app.try_state::<AppState>()
        .and_then(|state| state.shortcut_errors.read().ok().map(|e| e.clone()))
        .unwrap_or_default()
}

#[cfg(desktop)]
#[tauri::command]
pub async fn set_hotkey_enabled(
    app: AppHandle,
    hotkey_type: HotkeyType,
    enabled: bool,
) -> Result<(), String> {
    let setting = hotkey_type.local_only_setting();
    let mut hotkey: HotkeyConfig =
        get_setting_from_store(&app, setting, hotkey_type.default_hotkey());
    hotkey.enabled = enabled;

    persist_setting(&app, setting, &hotkey).map_err(|error| format!("{error:#}"))?;
    log::info!(
        "Set {} hotkey enabled: {}",
        hotkey_type.display_name(),
        enabled
    );
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn set_hotkey_enabled(
    _app: AppHandle,
    _hotkey_type: HotkeyType,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

// ============================================================================
// SETTINGS CRUD COMMANDS
// ============================================================================

#[cfg(desktop)]
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(AppSettings {
        toggle_hotkey: get_setting_from_store(
            &app,
            LocalOnlySetting::ToggleHotkey,
            HotkeyConfig::default_toggle(),
        ),
        hold_hotkey: get_setting_from_store(
            &app,
            LocalOnlySetting::HoldHotkey,
            HotkeyConfig::default_hold(),
        ),
        paste_last_hotkey: get_setting_from_store(
            &app,
            LocalOnlySetting::PasteLastHotkey,
            HotkeyConfig::default_paste_last(),
        ),
        selected_mic_id: get_setting_from_store(&app, LocalOnlySetting::SelectedMicId, None),
        sound_enabled: get_setting_from_store(&app, LocalOnlySetting::SoundEnabled, true),
        cleanup_prompt_sections: get_setting_from_store(
            &app,
            LocalOnlySetting::CleanupPromptSections,
            None,
        ),
        auto_mute_audio: get_setting_from_store(&app, LocalOnlySetting::AutoMuteAudio, false),
        openai_api_key: get_setting_from_store(&app, LocalOnlySetting::OpenaiApiKey, None),
        llm_formatting_enabled: get_setting_from_store(
            &app,
            LocalOnlySetting::LlmFormattingEnabled,
            true,
        ),
        send_active_app_context_enabled: get_setting_from_store(
            &app,
            LocalOnlySetting::SendActiveAppContextEnabled,
            false,
        ),
    })
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn get_settings(_app: AppHandle) -> Result<AppSettings, String> {
    Ok(AppSettings::default())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_hotkey(
    app: AppHandle,
    hotkey_type: HotkeyType,
    config: HotkeyConfig,
) -> Result<(), SettingsError> {
    let settings = get_settings(app.clone()).map_err(|e| SettingsError::StoreError(e.clone()))?;
    if let Some(error) = check_hotkey_conflict(&config, &settings, hotkey_type) {
        return Err(error);
    }
    let setting = hotkey_type.local_only_setting();
    persist_setting(&app, setting, &config)
        .map_err(|error| SettingsError::StoreError(format!("{error:#}")))?;
    log::info!(
        "Updated {} hotkey to: {}",
        hotkey_type.display_name(),
        config.to_shortcut_string()
    );
    // Re-register shortcuts so the new hotkey takes effect immediately
    crate::do_register_shortcuts(&app);
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_hotkey(
    _app: AppHandle,
    _hotkey_type: HotkeyType,
    _config: HotkeyConfig,
) -> Result<(), SettingsError> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_selected_mic(app: AppHandle, mic_id: Option<String>) -> Result<(), String> {
    persist_setting(&app, LocalOnlySetting::SelectedMicId, &mic_id)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated selected microphone: {mic_id:?}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_selected_mic(_app: AppHandle, _mic_id: Option<String>) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_sound_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    persist_setting(&app, LocalOnlySetting::SoundEnabled, &enabled)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated sound enabled: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_sound_enabled(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_cleanup_prompt_sections(
    app: AppHandle,
    sections: Option<CleanupPromptSections>,
) -> Result<(), String> {
    persist_setting(&app, LocalOnlySetting::CleanupPromptSections, &sections)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated cleanup prompt sections");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_cleanup_prompt_sections(
    _app: AppHandle,
    _sections: Option<CleanupPromptSections>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_auto_mute_audio(app: AppHandle, enabled: bool) -> Result<(), String> {
    persist_setting(&app, LocalOnlySetting::AutoMuteAudio, &enabled)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated auto mute audio: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_auto_mute_audio(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_openai_api_key(
    app: AppHandle,
    api_key: Option<String>,
) -> Result<(), String> {
    persist_setting(&app, LocalOnlySetting::OpenaiApiKey, &api_key)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated OpenAI API key");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_openai_api_key(
    _app: AppHandle,
    _api_key: Option<String>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_llm_formatting_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    persist_setting(&app, LocalOnlySetting::LlmFormattingEnabled, &enabled)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("LLM formatting enabled: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_llm_formatting_enabled(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn update_send_active_app_context_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    persist_setting(
        &app,
        LocalOnlySetting::SendActiveAppContextEnabled,
        &enabled,
    )
    .map_err(|error| format!("{error:#}"))?;

    if let Err(error) = reconcile_focus_watcher_enabled_state(&app, enabled) {
        log::warn!(
            "Failed to reconcile focus watcher while updating send_active_app_context_enabled: {error:#}"
        );
    }

    log::info!("Send active app context enabled: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_send_active_app_context_enabled(
    _app: AppHandle,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(app: AppHandle) -> Result<(), String> {
    persist_setting(
        &app,
        LocalOnlySetting::ToggleHotkey,
        &HotkeyConfig::default_toggle(),
    )
    .map_err(|error| format!("{error:#}"))?;
    persist_setting(
        &app,
        LocalOnlySetting::HoldHotkey,
        &HotkeyConfig::default_hold(),
    )
    .map_err(|error| format!("{error:#}"))?;
    persist_setting(
        &app,
        LocalOnlySetting::PasteLastHotkey,
        &HotkeyConfig::default_paste_last(),
    )
    .map_err(|error| format!("{error:#}"))?;
    log::info!("Reset all hotkeys to defaults");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(_app: AppHandle) -> Result<(), String> {
    Ok(())
}
