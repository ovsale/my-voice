//! Type-safe event system for inter-window communication.
//!
//! Events are broadcast to all windows via Tauri's event system.
//! This module provides constants and types for event names and payloads.
//!
//! IMPORTANT: Event names and payload types must match the TypeScript side.
//! See: src/lib/events.ts

use serde::Serialize;

// =============================================================================
// Event Names - Must match src/lib/events.ts
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventName {
    /// Rust → All: Recording started (hotkey)
    RecordingStart,
    /// Rust → All: Recording stopped (hotkey)
    RecordingStop,
    /// Rust → All: Prepare for recording (mic warmup)
    PrepareRecording,
    /// Main → Overlay: Settings changed, refetch needed
    SettingsChanged,
    /// Rust → All: History changed
    HistoryChanged,
    /// Rust → All: Recording status changed
    RecordingStatusChanged,
    /// Rust → All: Active app context updates
    ActiveAppContextChanged,
}

impl EventName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecordingStart => "recording-start",
            Self::RecordingStop => "recording-stop",
            Self::PrepareRecording => "prepare-recording",
            Self::SettingsChanged => "settings-changed",
            Self::HistoryChanged => "history-changed",
            Self::RecordingStatusChanged => "recording-status-changed",
            Self::ActiveAppContextChanged => "active-app-context-changed",
        }
    }
}

// =============================================================================
// Event Payloads
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingStatus {
    Idle,
    Recording,
    Processing,
    Error,
}
