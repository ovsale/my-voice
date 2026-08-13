import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ActiveAppContextSnapshot } from "./activeAppContext";
import type { RecordingStatus } from "./events";

export * from "./activeAppContext";

import { AppEvents, emitEvent, listenEvent } from "./events";

// =============================================================================
// Types
// =============================================================================

interface TypeTextResult {
	success: boolean;
	error?: string;
}

export interface AudioDeviceInfo {
	id: string;
	name: string;
}

export interface HotkeyConfig {
	modifiers: string[];
	key: string;
	enabled: boolean;
}

export interface ShortcutErrors {
	toggle_error: string | null;
	hold_error: string | null;
	paste_last_error: string | null;
}

export interface ShortcutRegistrationResult {
	toggle_registered: boolean;
	hold_registered: boolean;
	paste_last_registered: boolean;
	errors: ShortcutErrors;
}

export type TranscriptionStatus = "ok" | "failed";

export interface HistoryEntry {
	id: string;
	timestamp: string;
	text: string;
	raw_text: string;
	active_app_context?: ActiveAppContextSnapshot | null;
	status: TranscriptionStatus;
	error?: string | null;
}

// =============================================================================
// Export/Import Types
// =============================================================================

/** Strategy for importing history entries */
export type HistoryImportStrategy =
	| "replace"
	| "merge_append"
	| "merge_deduplicate";

/** Result of a history import operation */
export interface HistoryImportResult {
	success: boolean;
	entries_imported: number | null;
	entries_skipped: number | null;
}

/** Warning from best-effort runtime setting application after import/reset. */
export type RuntimeApplyWarningCode =
	| "focus_watcher_reconcile_failed"
	| "prompt_sections_sync_failed"
	| "llm_formatting_sync_failed";

export type RuntimeApplySettingKey = keyof Pick<
	AppSettings,
	| "send_active_app_context_enabled"
	| "cleanup_prompt_sections"
	| "llm_formatting_enabled"
>;

export type RuntimeApplyAction =
	| "focus_watcher_enabled"
	| "focus_watcher_disabled"
	| "prompt_sections_synced"
	| "llm_formatting_synced";

/** Warning from best-effort runtime setting application after import/reset. */
export interface RuntimeApplyWarning {
	code: RuntimeApplyWarningCode;
	message: string;
	setting_key: RuntimeApplySettingKey;
}

/** Runtime action that was successfully applied after import/reset. */
export interface RuntimeActionApplied {
	action: RuntimeApplyAction;
	setting_key: RuntimeApplySettingKey;
}

/** Runtime setting application summary for settings import. */
export interface ImportSettingsOutcome {
	warnings: RuntimeApplyWarning[];
	runtime_actions_applied: RuntimeActionApplied[];
}

/** Runtime setting application summary for factory reset. */
export interface FactoryResetOutcome {
	warnings: RuntimeApplyWarning[];
	runtime_actions_applied: RuntimeActionApplied[];
}

/** Detected file type from import */
export type DetectedFileType = "settings" | "history" | "unknown";

/** Prompt section names */
export type PromptSectionName = "main" | "advanced" | "dictionary";

/**
 * Mode of prompt: auto (let server optimize) or manual (custom content).
 * Discriminated union ensures content only exists for manual mode.
 */
export type PromptMode = { mode: "auto" } | { mode: "manual"; content: string };

/**
 * Configuration for a single prompt section.
 * Two-layer structure:
 * - enabled: Whether the section is active
 * - mode: The prompt mode (auto or manual with content)
 */
export interface PromptSection {
	enabled: boolean;
	mode: PromptMode;
}

export interface CleanupPromptSections {
	main: PromptSection;
	advanced: PromptSection;
	dictionary: PromptSection;
}

export interface SttProvider {
	name: string;
	base_url: string;
	model: string;
	api_key: string;
	/** "multipart" (OpenAI) or "json" (OpenRouter) */
	request_format: string;
	extra_body: string | null;
}

export interface AppSettings {
	toggle_hotkey: HotkeyConfig;
	hold_hotkey: HotkeyConfig;
	paste_last_hotkey: HotkeyConfig;
	selected_mic_id: string | null;
	sound_enabled: boolean;
	cleanup_prompt_sections: CleanupPromptSections | null;
	volume_reduction_percent: number;
	stt_providers: SttProvider[];
	active_stt_provider_index: number;
	stt_prompt: string | null;
	paste_prefix: string | null;
	openai_api_key: string | null;
	/** LLM formatting enabled (true = format with LLM, false = raw transcription) */
	llm_formatting_enabled: boolean;
	/** Send active app context to server for prompt injection */
	send_active_app_context_enabled: boolean;
	/** Overlay size in pixels (24–128, default 48) */
	overlay_size_px: number;
}

// ============================================================================
// Hotkey validation helpers (for immediate UI feedback)
// Rust provides the same validation as a safety net on save
// ============================================================================

/**
 * Check if two hotkey configs are equivalent (case-insensitive comparison)
 */
export function hotkeyIsSameAs(a: HotkeyConfig, b: HotkeyConfig): boolean {
	if (a.key.toLowerCase() !== b.key.toLowerCase()) return false;
	if (a.modifiers.length !== b.modifiers.length) return false;
	return a.modifiers.every((mod) =>
		b.modifiers.some((other) => mod.toLowerCase() === other.toLowerCase()),
	);
}

export type HotkeyType = "toggle" | "hold" | "paste_last";

const HOTKEY_LABELS: Record<HotkeyType, string> = {
	toggle: "toggle",
	hold: "hold",
	paste_last: "paste last",
};

/**
 * Validate that a hotkey doesn't conflict with other hotkeys
 * Returns error message if invalid, null if valid
 * Used for immediate UI feedback - Rust provides the same validation as a safety net
 */
export function validateHotkeyNotDuplicate(
	newHotkey: HotkeyConfig,
	allHotkeys: {
		toggle: HotkeyConfig;
		hold: HotkeyConfig;
		paste_last: HotkeyConfig;
	},
	excludeType: HotkeyType,
): string | null {
	for (const [type, existing] of Object.entries(allHotkeys)) {
		if (type !== excludeType && hotkeyIsSameAs(newHotkey, existing)) {
			return `This shortcut is already used for the ${HOTKEY_LABELS[type as HotkeyType]} hotkey`;
		}
	}
	return null;
}

// =============================================================================
// Tauri API
// =============================================================================

export const tauriAPI = {
	async typeText(text: string): Promise<TypeTextResult> {
		try {
			await invoke("type_text", { text });
			return { success: true };
		} catch (error) {
			return { success: false, error: String(error) };
		}
	},

	// Recording control (Rust backend handles mic → WAV → Whisper → paste)
	async startRecording(deviceId?: string): Promise<void> {
		return invoke("start_recording", { deviceId });
	},

	async stopRecording(): Promise<void> {
		return invoke("stop_recording");
	},

	async getRecordingStatus(): Promise<RecordingStatus> {
		return invoke("get_recording_status");
	},

	async onRecordingStatusChanged(
		callback: (status: RecordingStatus) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.recordingStatusChanged, callback);
	},

	// Hotkey event listeners
	async onStartRecording(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.recordingStart, callback);
	},

	async onStopRecording(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.recordingStop, callback);
	},

	async onPrepareRecording(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.prepareRecording, callback);
	},

	// Settings
	async getSettings(): Promise<AppSettings> {
		return invoke("get_settings");
	},

	async updateToggleHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hotkey", { hotkeyType: "toggle", config: hotkey });
	},

	async updateHoldHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hotkey", { hotkeyType: "hold", config: hotkey });
	},

	async updatePasteLastHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hotkey", {
			hotkeyType: "paste_last",
			config: hotkey,
		});
	},

	async updateSelectedMic(micId: string | null): Promise<void> {
		return invoke("update_selected_mic", { micId });
	},

	async updateSoundEnabled(enabled: boolean): Promise<void> {
		return invoke("update_sound_enabled", { enabled });
	},

	async updateCleanupPromptSections(
		sections: CleanupPromptSections | null,
	): Promise<void> {
		return invoke("update_cleanup_prompt_sections", { sections });
	},

	async updateVolumeReductionPercent(percent: number): Promise<void> {
		return invoke("update_volume_reduction_percent", { percent });
	},

	async updateSttProviders(providers: SttProvider[]): Promise<void> {
		return invoke("update_stt_providers", { providers });
	},

	async updateActiveSttProviderIndex(index: number): Promise<void> {
		return invoke("update_active_stt_provider_index", { index });
	},

	async updateSttPrompt(prompt: string | null): Promise<void> {
		return invoke("update_stt_prompt", { prompt });
	},

	async updatePastePrefix(prefix: string | null): Promise<void> {
		return invoke("update_paste_prefix", { prefix });
	},

	async updateOpenaiApiKey(apiKey: string | null): Promise<void> {
		return invoke("update_openai_api_key", { apiKey });
	},

	async updateLLMFormattingEnabled(enabled: boolean): Promise<void> {
		return invoke("update_llm_formatting_enabled", { enabled });
	},

	async updateSendActiveAppContextEnabled(enabled: boolean): Promise<void> {
		return invoke("update_send_active_app_context_enabled", { enabled });
	},

	async updateOverlaySizePx(size: number): Promise<void> {
		return invoke("update_overlay_size_px", { size });
	},

	async listNativeAudioDevices(): Promise<AudioDeviceInfo[]> {
		return invoke("list_native_mic_devices");
	},

	async isAudioMuteSupported(): Promise<boolean> {
		return invoke("is_audio_mute_supported");
	},

	async resetHotkeysToDefaults(): Promise<void> {
		return invoke("reset_hotkeys_to_defaults");
	},

	async registerShortcuts(): Promise<ShortcutRegistrationResult> {
		return invoke("register_shortcuts");
	},

	async unregisterShortcuts(): Promise<void> {
		return invoke("unregister_shortcuts");
	},

	async getShortcutErrors(): Promise<ShortcutErrors> {
		return invoke("get_shortcut_errors");
	},

	async setHotkeyEnabled(
		hotkeyType: "toggle" | "hold" | "paste_last",
		enabled: boolean,
	): Promise<void> {
		return invoke("set_hotkey_enabled", { hotkeyType, enabled });
	},

	// History
	async addHistoryEntry(
		text: string,
		rawText: string,
		activeAppContext?: ActiveAppContextSnapshot | null,
	): Promise<HistoryEntry> {
		return invoke("add_history_entry", { text, rawText, activeAppContext });
	},

	async getHistory(limit?: number): Promise<HistoryEntry[]> {
		return invoke("get_history", { limit });
	},

	async deleteHistoryEntry(id: string): Promise<boolean> {
		return invoke("delete_history_entry", { id });
	},

	async clearHistory(): Promise<void> {
		return invoke("clear_history");
	},

	// Re-transcription of the last saved recording
	async getLastRecordingEntryId(): Promise<string | null> {
		return invoke("get_last_recording_entry_id");
	},

	async retranscribeLast(): Promise<void> {
		return invoke("retranscribe_last");
	},

	// Overlay API
	async resizeOverlay(width: number, height: number): Promise<void> {
		return invoke("resize_overlay", { width, height });
	},

	async startDragging(): Promise<void> {
		const window = getCurrentWindow();
		return window.startDragging();
	},

	// History sync between windows
	async emitHistoryChanged(): Promise<void> {
		return emitEvent(AppEvents.historyChanged);
	},

	async onHistoryChanged(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.historyChanged, callback);
	},

	// Settings sync between windows (main -> overlay)
	async emitSettingsChanged(): Promise<void> {
		return emitEvent(AppEvents.settingsChanged);
	},

	async onSettingsChanged(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.settingsChanged, callback);
	},

	// Active app context
	async onActiveAppContextChanged(
		callback: (payload: ActiveAppContextSnapshot) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.activeAppContextChanged, callback);
	},

	async activeAppGetCurrentContext(): Promise<ActiveAppContextSnapshot> {
		return invoke("active_app_get_current_context");
	},

	// Export/Import API
	async generateSettingsExport(): Promise<string> {
		return invoke("generate_settings_export");
	},

	async generateHistoryExport(): Promise<string> {
		return invoke("generate_history_export");
	},

	/** Generate prompt exports as markdown content. Returns map of section name -> markdown content. */
	async generatePromptExports(): Promise<Record<PromptSectionName, string>> {
		return invoke("generate_prompt_exports");
	},

	/** Parse a prompt file and extract section name and content from HTML comment header. */
	async parsePromptFile(content: string): Promise<[PromptSectionName, string]> {
		return invoke("parse_prompt_file", { content });
	},

	/** Import a prompt into the specified section. */
	async importPrompt(
		section: PromptSectionName,
		content: string,
	): Promise<void> {
		return invoke("import_prompt", { section, content });
	},

	async detectExportFileType(content: string): Promise<DetectedFileType> {
		return invoke("detect_export_file_type", { content });
	},

	async importSettings(content: string): Promise<ImportSettingsOutcome> {
		return invoke("import_settings", { content });
	},

	async importHistory(
		content: string,
		strategy: HistoryImportStrategy,
	): Promise<HistoryImportResult> {
		return invoke("import_history", { content, strategy });
	},

	async factoryReset(): Promise<FactoryResetOutcome> {
		return invoke("factory_reset");
	},
};
