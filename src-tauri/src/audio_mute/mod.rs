//! System audio mute control for voice dictation.
//!
//! This module provides a minimal trait interface for controlling system audio,
//! making it easy to swap implementations or migrate to a cross-platform library.

use std::fmt;
use std::sync::Mutex;

// Platform-specific implementations
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod stub;
#[cfg(target_os = "windows")]
mod windows;

/// Error type for audio control operations
#[derive(Debug)]
#[allow(dead_code)] // Variants used on Windows/macOS, not Linux
pub enum AudioControlError {
    /// Platform-specific initialization failed
    InitializationFailed(String),
    /// Failed to get audio property
    GetPropertyFailed(String),
    /// Failed to set audio property
    SetPropertyFailed(String),
    /// Platform not supported
    NotSupported,
}

impl fmt::Display for AudioControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializationFailed(msg) => write!(f, "Audio init failed: {msg}"),
            Self::GetPropertyFailed(msg) => write!(f, "Failed to get audio property: {msg}"),
            Self::SetPropertyFailed(msg) => write!(f, "Failed to set audio property: {msg}"),
            Self::NotSupported => write!(f, "Audio control not supported on this platform"),
        }
    }
}

impl std::error::Error for AudioControlError {}

/// Trait for controlling system audio mute and volume state.
///
/// This minimal interface allows easy migration to a cross-platform library
/// by just swapping the implementation behind `create_controller()`.
pub trait SystemAudioControl: Send + Sync {
    /// Check if system audio is muted
    fn is_muted(&self) -> Result<bool, AudioControlError>;

    /// Set system mute state
    fn set_muted(&self, muted: bool) -> Result<(), AudioControlError>;

    /// Get the current system volume (0.0 = silent, 1.0 = max)
    fn get_volume(&self) -> Result<f32, AudioControlError>;

    /// Set the system volume (0.0 = silent, 1.0 = max)
    fn set_volume(&self, volume: f32) -> Result<(), AudioControlError>;
}

/// Check if audio mute is supported on this platform.
pub fn is_supported() -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        true
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

/// Create a platform-appropriate audio controller.
///
/// Returns a boxed trait object that can control system audio.
/// On unsupported platforms, returns a stub that does nothing.
pub fn create_controller() -> Result<Box<dyn SystemAudioControl>, AudioControlError> {
    #[cfg(target_os = "windows")]
    {
        windows::WindowsAudioController::new().map(|c| Box::new(c) as Box<dyn SystemAudioControl>)
    }

    #[cfg(target_os = "macos")]
    {
        macos::MacOSAudioController::new().map(|c| Box::new(c) as Box<dyn SystemAudioControl>)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(Box::new(stub::StubAudioController::new()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VolumeReductionState {
    /// Not currently reducing volume
    Idle,
    /// We reduced the volume; stores the original volume to restore
    ReducedByUs { original_volume: f32 },
    /// Audio was already muted by user, don't touch on restore
    WasAlreadyMutedByUser,
}

/// Manages reducing/restoring system audio volume during recording.
pub struct AudioMuteManager {
    controller: Box<dyn SystemAudioControl>,
    state: Mutex<VolumeReductionState>,
}

impl AudioMuteManager {
    pub fn new() -> Option<Self> {
        match create_controller() {
            Ok(controller) => Some(Self::from_controller(controller)),
            Err(e) => {
                log::warn!("Audio volume control not available: {e}");
                None
            }
        }
    }

    pub fn from_controller(controller: Box<dyn SystemAudioControl>) -> Self {
        Self {
            controller,
            state: Mutex::new(VolumeReductionState::Idle),
        }
    }

    /// Reduce system volume by the given percentage (0–100).
    /// 100 = full mute, 50 = half volume, 0 = no change.
    pub fn reduce_volume(&self, reduction_percent: u8) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();

        if !matches!(*state, VolumeReductionState::Idle) {
            return Ok(());
        }

        // If user already muted, don't touch anything
        if self.controller.is_muted().unwrap_or(false) {
            *state = VolumeReductionState::WasAlreadyMutedByUser;
            log::info!("System audio already muted by user, skipping volume reduction");
            return Ok(());
        }

        let original_volume = self.controller.get_volume().unwrap_or(1.0);

        if reduction_percent >= 100 {
            // Full mute
            self.controller.set_muted(true)?;
            log::info!("System audio muted for recording (was {:.0}%)", original_volume * 100.0);
        } else {
            // Perceptual (cubic) curve: makes the slider feel linear to human hearing.
            // Linear 50% reduction is only ~6dB (barely noticeable).
            // Cubic: slider 30% → volume ×0.34, slider 50% → volume ×0.13, slider 70% → volume ×0.03
            let linear = 1.0 - (f32::from(reduction_percent) / 100.0);
            let scale = linear * linear * linear;
            let target_volume = (original_volume * scale).max(0.0);
            self.controller.set_volume(target_volume)?;
            log::info!(
                "System audio reduced by {reduction_percent}% for recording ({:.0}% → {:.0}%)",
                original_volume * 100.0,
                target_volume * 100.0
            );
        }

        *state = VolumeReductionState::ReducedByUs { original_volume };
        Ok(())
    }

    /// Restore system volume to what it was before `reduce_volume`.
    pub fn restore_volume(&self) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();

        match *state {
            VolumeReductionState::ReducedByUs { original_volume } => {
                // Unmute first in case we used full mute
                if self.controller.is_muted().unwrap_or(false) {
                    self.controller.set_muted(false)?;
                }
                self.controller.set_volume(original_volume)?;
                log::info!("System audio restored to {:.0}%", original_volume * 100.0);
            }
            VolumeReductionState::WasAlreadyMutedByUser => {
                log::info!("System audio was already muted by user, leaving as-is");
            }
            VolumeReductionState::Idle => {}
        }

        *state = VolumeReductionState::Idle;
        Ok(())
    }
}

impl Drop for AudioMuteManager {
    fn drop(&mut self) {
        let state = self.state.lock().unwrap();
        if matches!(*state, VolumeReductionState::ReducedByUs { .. }) {
            drop(state);
            let _ = self.restore_volume();
        }
    }
}

#[cfg(test)]
#[path = "../tests/audio_mute_tests.rs"]
mod audio_mute_tests;
