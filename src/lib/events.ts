/**
 * Type-safe event system for inter-window communication.
 *
 * Events are broadcast to all windows via Tauri's event system.
 * This module provides type-safe wrappers around emit/listen.
 *
 * IMPORTANT: Event names and payload types must match the Rust side.
 * See: src-tauri/src/events.rs
 */

import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ActiveAppContextSnapshot } from "./activeAppContext";

// =============================================================================
// Event Names - Must match src-tauri/src/events.rs
// =============================================================================

export const AppEvents = {
	// Rust → All: Hotkey triggers
	recordingStart: "recording-start",
	recordingStop: "recording-stop",
	prepareRecording: "prepare-recording",

	// Rust → All: Recording status updates
	recordingStatusChanged: "recording-status-changed",

	// Main → Overlay: Settings changed, refetch needed
	settingsChanged: "settings-changed",

	// Rust → All: History changed
	historyChanged: "history-changed",

	// Rust → All: Active app context updates
	activeAppContextChanged: "active-app-context-changed",
} as const;

// =============================================================================
// Event Payloads - Must match src-tauri/src/events.rs
// =============================================================================

export type RecordingStatus = "idle" | "recording" | "processing" | "error";

export interface EventPayloads {
	[AppEvents.recordingStart]: undefined;
	[AppEvents.recordingStop]: undefined;
	[AppEvents.prepareRecording]: undefined;
	[AppEvents.recordingStatusChanged]: RecordingStatus;
	[AppEvents.settingsChanged]: undefined;
	[AppEvents.historyChanged]: undefined;
	[AppEvents.activeAppContextChanged]: ActiveAppContextSnapshot;
}

// =============================================================================
// Type-safe emit/listen functions
// =============================================================================

/**
 * Emit an event with type-safe payload.
 */
export function emitEvent<K extends keyof EventPayloads>(
	event: K,
	...args: EventPayloads[K] extends undefined ? [] : [EventPayloads[K]]
): Promise<void> {
	return emit(event, args[0] ?? {});
}

/**
 * Listen for an event with type-safe callback.
 */
export function listenEvent<K extends keyof EventPayloads>(
	event: K,
	callback: (payload: EventPayloads[K]) => void,
): Promise<UnlistenFn> {
	return listen<EventPayloads[K]>(event, (eventPayload) =>
		callback(eventPayload.payload),
	);
}
