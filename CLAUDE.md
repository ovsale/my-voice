# My Voice

Voice dictation desktop app. Hotkey → mic capture → configurable STT provider → optional LLM formatting → paste text into active window.

## Tech Stack

- **Desktop framework:** Tauri v2 (Rust backend + React frontend)
- **Frontend:** React 19, TypeScript, Mantine UI (dark theme), Zustand, Vite 7
- **Audio capture:** cpal 0.17 (CoreAudio on macOS, WASAPI on Windows)
- **Audio encoding:** Manual WAV (16-bit PCM), no hound at runtime
- **STT:** Configurable provider (OpenAI Whisper multipart, OpenRouter JSON+base64, or any compatible endpoint)
- **LLM:** OpenAI Chat Completions (`gpt-4o-mini`) for text cleanup
- **Text insertion:** arboard (clipboard) + enigo (Cmd+V simulation)
- **Global hotkeys:** tauri-plugin-global-shortcut
- **Overlay:** NSPanel (macOS) floating above fullscreen apps
- **Settings:** tauri-plugin-store (JSON)
- **History:** SQLite via rusqlite

## Project Structure

```
src/                          # Frontend (React + TypeScript)
  App.tsx                     # Main window (home + settings tabs)
  OverlayApp.tsx              # Floating 48x48 status overlay
  components/                 # Settings panels, history feed, etc.
  stores/                     # Zustand (recordingStore)
  hooks/                      # useSettings, useMutation
  lib/                        # tauri.ts (API bridge), events.ts, hotkey defaults

src-tauri/src/                # Rust backend
  lib.rs                      # App init, tray, hotkey registration & state machine
  recording.rs                # Pipeline orchestrator: mic → WAV → STT → LLM → paste
  audio_encoder.rs            # f32 PCM → WAV bytes (manual encoding)
  stt.rs                      # STT HTTP client (multipart + JSON formats)
  llm.rs                      # OpenAI Chat Completions HTTP client
  mic_capture/                # cpal-based mic capture with dedicated audio thread
    cpal_impl.rs              # Audio stream management via channels
  commands/
    text.rs                   # Clipboard paste (Cmd+V with layout-independent keycode)
    settings.rs               # Get/set settings, hotkey re-registration
    history.rs                # CRUD for transcription history
    overlay.rs                # Overlay window resize
    export_import.rs          # Settings/history export/import
  active_app_context/         # Detect focused window (macOS Accessibility API)
  audio_mute/                 # Reduce/mute system audio volume during recording
  history.rs                  # SQLite storage
  settings.rs                 # Settings schema (hotkeys, STT providers, prompts)
  state.rs                    # App state (ShortcutState machine)
  events.rs                   # Event names & payload types
```

## End-to-End Flow

1. **Hotkey** (lib.rs) → state machine transitions (Idle → Recording → Processing → Idle)
2. **Mic capture** (cpal_impl.rs) → f32 PCM samples pushed into shared Arc<Mutex<Vec<f32>>>
3. **WAV encode** (audio_encoder.rs) → 16-bit PCM WAV bytes
4. **STT API** (stt.rs) → multipart or JSON request to active provider, returns transcription text
5. **Trim** → leading/trailing whitespace stripped from transcription
6. **LLM format** (llm.rs) → optional cleanup via Chat Completions
7. **Prefix** → optional `paste_prefix` prepended (e.g. "🎙 ")
8. **Paste** (text.rs) → clipboard set → Cmd+V simulation (keycode 0x09, layout-independent)
9. **History** (history.rs) → save to SQLite, emit event
10. **Frontend** listens to `recording-status-changed` events, overlay shows status
11. **Cancel** → repeat hotkey during Processing aborts the pipeline (JoinHandle::abort)

## Key Conventions

- **Settings reactivity:** `persist_setting()` emits `SettingsChanged` after every save. Frontend `useSettings` hook auto-refreshes on this event — no manual emit or local state patching needed in components.
- **Event payloads:** Rust emits plain values (e.g. `RecordingStatus::Recording` → `"recording"`), not wrapped objects. Frontend `EventPayloads` types must match (e.g. `RecordingStatus`, not `{ status: RecordingStatus }`).
- Event names must match between `src-tauri/src/events.rs` and `src/lib/events.ts`
- **Hotkey strings:** normalized (lowercased + parts sorted alphabetically) for reliable matching regardless of modifier order
- **Hotkey recording:** `HotkeyInput` calls `unregisterShortcuts()` before capturing keys and `registerShortcuts()` after, so global shortcuts don't intercept the key combo being recorded
- **Text paste:** uses raw macOS keycode `Key::Other(0x09)` (kVK_ANSI_V) — layout-independent, works with any keyboard layout (Russian, etc.)
- **UTF-8 log truncation:** use `{:.120}` format (char-based), never `&s[..120]` (byte-based) — panics on multibyte chars
- Async tasks from non-async contexts use `tauri::async_runtime::spawn` (not `tokio::spawn`)
- Audio thread communicates via `std::sync::mpsc` channels (cpal::Stream is not Send on macOS)
- **Volume reduction:** `volume_reduction_percent` setting (0–100, default 0). Uses perceptual cubic curve (`scale = linear³`) so the slider feels linear to human hearing. At 100% uses system mute; otherwise adjusts volume via `kAudioDevicePropertyVolumeScalar` (macOS) / `SetMasterVolumeLevelScalar` (Windows). Original volume is saved and restored after recording.
- **STT providers:** `stt_providers` is an array of `SttProvider` objects (name, base_url, model, api_key, request_format, extra_body). `active_stt_provider_index` selects which one to use. Two request formats: `"multipart"` (OpenAI-native form upload) and `"json"` (OpenRouter-style JSON with base64 audio in `input_audio.data`). Extra body params are merged into the request (form fields for multipart, flattened JSON keys for json format).
- **STT prompt:** Global `stt_prompt` setting sent with every STT request as context hint. Supported by OpenAI Whisper; may be ignored by other providers (e.g. Groq via OpenRouter).
- **Paste prefix:** Optional `paste_prefix` prepended to every transcription before pasting.
- **Pipeline cancellation:** Repeating the toggle hotkey during Processing state aborts the running async task via `JoinHandle::abort()`. State machine tracks Processing separately and resets to Idle on cancel or pipeline completion.
- **Re-transcription:** Every recording's WAV is saved to `last_recording.wav` (app data dir, single slot, overwritten by the next recording) before the STT call; `last_recording.json` links it to the history entry that owns it. On STT failure (or empty result) a `status: failed` history entry is created with the error text. The `retranscribe_last` command re-runs STT → LLM → prefix on the saved clip with *current* settings (so switching providers applies) and updates the owning entry in place — no Cmd+V paste (the app window is focused). Exposed in the history item context menu ("Re-transcribe"), shown only on the entry linked to the saved clip. A failed re-transcribe never wipes previously successful text (`mark_entry_failed` skips entries with text).

## Working with Claude

- **No plan mode** unless explicitly asked. Default: discuss the task first, then implement directly.
- Workflow: discuss what needs to be done → agree on approach → implement. No autonomous planning.

## Build Commands

```bash
pnpm tauri dev          # Dev with hot reload
pnpm tauri build --debug  # Fast debug build (.app + .dmg)
pnpm tauri build        # Release build (optimized, slow)
```

## Hotkeys (defaults)

- `Ctrl+Alt+Space` — Toggle recording (press to start, press again to stop)
- `Ctrl+Alt+Backquote` — Hold to record, release to stop
- `Ctrl+Alt+Period` — Paste last transcription
