use std::sync::{Arc, Mutex};

use super::{AudioControlError, AudioMuteManager, SystemAudioControl};

#[derive(Debug, Default)]
struct FakeAudioControllerState {
    is_muted: bool,
    volume: f32,
    set_muted_calls: Vec<bool>,
    set_volume_calls: Vec<f32>,
    is_muted_error: Option<String>,
}

#[derive(Clone)]
struct FakeAudioController {
    state: Arc<Mutex<FakeAudioControllerState>>,
}

impl FakeAudioController {
    fn new(state: Arc<Mutex<FakeAudioControllerState>>) -> Self {
        Self { state }
    }
}

impl SystemAudioControl for FakeAudioController {
    fn is_muted(&self) -> Result<bool, AudioControlError> {
        let state = self.state.lock().unwrap();
        if let Some(error_message) = &state.is_muted_error {
            return Err(AudioControlError::GetPropertyFailed(error_message.clone()));
        }
        Ok(state.is_muted)
    }

    fn set_muted(&self, muted: bool) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();
        state.set_muted_calls.push(muted);
        state.is_muted = muted;
        Ok(())
    }

    fn get_volume(&self) -> Result<f32, AudioControlError> {
        let state = self.state.lock().unwrap();
        Ok(state.volume)
    }

    fn set_volume(&self, volume: f32) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();
        state.set_volume_calls.push(volume);
        state.volume = volume;
        Ok(())
    }
}

#[test]
fn reduce_volume_full_mute_uses_set_muted() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.8,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    manager.reduce_volume(100).unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.set_muted_calls, vec![true]);
    assert!(s.set_volume_calls.is_empty());
}

#[test]
fn reduce_volume_partial_sets_volume_with_cubic_curve() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.8,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    manager.reduce_volume(50).unwrap();

    let s = state.lock().unwrap();
    assert!(s.set_muted_calls.is_empty());
    assert_eq!(s.set_volume_calls.len(), 1);
    // Cubic curve: 0.5^3 = 0.125, target = 0.8 * 0.125 = 0.1
    assert!((s.set_volume_calls[0] - 0.1).abs() < 0.01);
}

#[test]
fn reduce_volume_zero_is_noop() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.8,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    // reduce_volume(0) should not be called in practice (lib.rs guards with > 0),
    // but if called, cubic curve: 1.0^3 = 1.0, target = 0.8 * 1.0 = 0.8
    manager.reduce_volume(0).unwrap();

    let s = state.lock().unwrap();
    assert!(s.set_muted_calls.is_empty());
    assert_eq!(s.set_volume_calls.len(), 1);
    assert!((s.set_volume_calls[0] - 0.8).abs() < 0.01);
}

#[test]
fn restore_volume_after_full_mute() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.7,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    manager.reduce_volume(100).unwrap();
    manager.restore_volume().unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.set_muted_calls, vec![true, false]); // mute then unmute
    assert_eq!(s.set_volume_calls.len(), 1);
    assert!((s.set_volume_calls[0] - 0.7).abs() < 0.01); // restored to original
}

#[test]
fn restore_volume_after_partial_reduction() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.6,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    manager.reduce_volume(80).unwrap();
    manager.restore_volume().unwrap();

    let s = state.lock().unwrap();
    assert!(s.set_muted_calls.is_empty()); // no mute calls for partial reduction
    assert_eq!(s.set_volume_calls.len(), 2);
    // Cubic curve: 0.2^3 = 0.008, target = 0.6 * 0.008 = 0.0048
    assert!((s.set_volume_calls[0] - 0.0048).abs() < 0.01);
    assert!((s.set_volume_calls[1] - 0.6).abs() < 0.01); // restored
}

#[test]
fn skips_reduction_when_already_muted_by_user() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        is_muted: true,
        volume: 0.5,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    manager.reduce_volume(100).unwrap();
    manager.restore_volume().unwrap();

    let s = state.lock().unwrap();
    assert!(s.set_muted_calls.is_empty());
    assert!(s.set_volume_calls.is_empty());
}

#[test]
fn reduce_is_idempotent() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.8,
        ..Default::default()
    }));
    let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));

    manager.reduce_volume(50).unwrap();
    manager.reduce_volume(50).unwrap(); // second call should be ignored

    let s = state.lock().unwrap();
    assert_eq!(s.set_volume_calls.len(), 1);
}

#[test]
fn drop_restores_volume() {
    let state = Arc::new(Mutex::new(FakeAudioControllerState {
        volume: 0.9,
        ..Default::default()
    }));

    {
        let manager = AudioMuteManager::from_controller(Box::new(FakeAudioController::new(state.clone())));
        manager.reduce_volume(100).unwrap();
    } // drop here

    let s = state.lock().unwrap();
    assert_eq!(s.set_muted_calls, vec![true, false]); // muted, then unmuted on drop
    assert!((s.set_volume_calls[0] - 0.9).abs() < 0.01); // restored on drop
}
