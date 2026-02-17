# My Voice

Voice dictation desktop app. Hotkey → mic capture → OpenAI Whisper STT → optional LLM formatting → paste text into active window.

## Tech Stack

- **Desktop framework:** Tauri v2 (Rust backend + React frontend)
- **Frontend:** React 19, TypeScript, Mantine UI (dark theme), Zustand, Vite 7
- **Audio capture:** cpal 0.17 (CoreAudio on macOS, WASAPI on Windows)
- **Audio encoding:** Manual WAV (16-bit PCM), no hound at runtime
- **STT:** OpenAI Whisper API (`/v1/audio/transcriptions`)
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
  stt.rs                      # OpenAI Whisper HTTP client
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
  audio_mute/                 # Mute system audio during recording
  history.rs                  # SQLite storage
  settings.rs                 # Settings schema (hotkeys, API key, prompts)
  state.rs                    # App state (ShortcutState machine)
  events.rs                   # Event names & payload types
```

## End-to-End Flow

1. **Hotkey** (lib.rs) → state machine transitions (Idle → Recording → Processing → Idle)
2. **Mic capture** (cpal_impl.rs) → f32 PCM samples pushed into shared Arc<Mutex<Vec<f32>>>
3. **WAV encode** (audio_encoder.rs) → 16-bit PCM WAV bytes
4. **Whisper API** (stt.rs) → multipart POST, returns transcription text
5. **LLM format** (llm.rs) → optional cleanup via Chat Completions
6. **Paste** (text.rs) → clipboard set → Cmd+V simulation (keycode 0x09, layout-independent)
7. **History** (history.rs) → save to SQLite, emit event
8. **Frontend** listens to `recording-status-changed` events, overlay shows status

## Key Conventions

- Rust events emitted via `app.emit()` with snake_case payload serialization
- Frontend listens via typed `listenEvent()` wrapper in `lib/events.ts`
- Event names must match between `src-tauri/src/events.rs` and `src/lib/events.ts`
- Hotkey strings are normalized (lowercased + sorted) for reliable matching
- Text paste uses raw macOS keycode `Key::Other(0x09)` (kVK_ANSI_V) to work with any keyboard layout
- Async tasks from non-async contexts use `tauri::async_runtime::spawn` (not `tokio::spawn`)
- Audio thread communicates via `std::sync::mpsc` channels (cpal::Stream is not Send on macOS)

## Build Commands

```bash
pnpm tauri dev          # Dev with hot reload
pnpm tauri build --debug  # Fast debug build (.app + .dmg)
pnpm tauri build        # Release build (optimized, slow)
```

## Hotkeys (defaults)

- `Ctrl+Shift+H` — Toggle recording (press to start, press again to stop)
- `Ctrl+Alt+Backquote` — Hold to record, release to stop
- `Ctrl+Alt+Period` — Paste last transcription
