# My Voice

Voice dictation desktop app. Hotkey → mic capture → configurable STT provider → optional LLM formatting → paste text into active window.

## Main communication and security rules

**Voice input (🎤) may be inaccurate.** Messages with 🎤 are speech-transcribed. For any critical action (commit, release build, deleting data, overwriting the installed app) — ALWAYS ask for confirmation.

**NEVER commit or push without explicit permission.** "Build" does NOT mean "commit". Each action requires separate confirmation in the current message.

**NEVER build, install, or launch the app without explicit permission.** No `pnpm tauri build`, `pnpm tauri dev`, opening the DMG, or launching the app unless the user explicitly asks in the current message — earlier mentions of an upcoming release do not count.

**NEVER delete, remove, or overwrite user data** in the app data directory (`settings.json`, `history.json`, `last_recording.wav`) without EXPLICIT permission. Dictation history and settings are irreplaceable.

**Do NOT read or print API keys** — they live in the settings store (`settings.json` in app data) and in `.env` files of neighboring projects.

**Do NOT offer predefined multiple-choice questions.** Ask questions as plain text — the user answers in text.

**When a mistake is discovered — discuss before redoing.** If it turns out something was done incorrectly (wrong assumptions, a bug the user found, new facts), do NOT rush to fix the code. First present the diagnosis and the proposed fix, discuss, and only after agreement — implement.

## Coding rules

**Workflow: understand first, then implement.** Do NOT jump to editing code. First discuss the task with the user, understand the problem, agree on the approach — then implement. Ask questions if anything is unclear.

**Do NOT use plan mode.** The user prefers direct discussion and implementation.

**Implement only what was agreed.** No unrequested extras (notifications, UI elements, features) beyond the discussed scope.

### Code style

**No JS/TS getter/setter syntax** (`get x()`, `set x()`). Use explicit methods: `getSleepUntil()`, `setSleepUntil(value)`.

**Top-down code order (newspaper style).** Main class/function first, helper functions below. The caller comes before the callee. High-level logic at the top of the file, implementation details at the bottom. Applies to both TypeScript and Rust.

**Always use curly braces.** No one-liners for `if`, `for`, `while`, etc. Always use `{ }` even for single-line bodies. Lines are free — readability is not.

**`else if` only for flat chains.** Use `else if` only when checking values at the same level (like a switch). When branches have nested logic inside, use `else { if ... }` — otherwise the structure isn't clear at a glance.

**Formatting is handled by Biome** (`pnpm lint`): tabs, double quotes. Don't hand-reformat existing code; match the file you're in.

**`const` only for true constants.** Use `const` for declared constants (module-level values, config). For local variables, `let` is fine — don't mechanically replace every `let` with `const` just because the variable isn't reassigned.

**No Defensive Coding.** Don't wrap code in try/catch "just in case". If something can't fail — don't catch. If it can fail — handle the specific error meaningfully or let it crash. Silent swallowing of exceptions hides bugs. Every catch must have a clear reason for existing.

### Project files — rules

- **`CLAUDE.md`** — current implemented state. Do NOT update after every change — only when the user explicitly asks. If the user asks to commit but forgets to update — remind them. Keep it short: only the main ideas and invariants — details live in the code.

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
- **History:** JSON file (`history.json` in app data, atomic writes via tempfile)

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
  history.rs                  # JSON storage; entries carry ok/failed/processing status
  settings.rs                 # Settings schema (hotkeys, STT providers, prompts)
  state.rs                    # App state (ShortcutState machine)
  events.rs                   # Event names & payload types
```

## End-to-End Flow

1. **Hotkey** (lib.rs) → state machine transitions (Idle → Recording → Processing → Idle)
2. **Mic capture** (cpal_impl.rs) → f32 PCM samples pushed into shared Arc<Mutex<Vec<f32>>>
3. **WAV encode** (audio_encoder.rs) → 16-bit PCM WAV bytes, saved to `last_recording.wav` before any network call
4. **History entry** created immediately with `processing` status ("Transcribing…" in the feed)
5. **STT API** (stt.rs) → multipart / json / gemini request to active provider, returns transcription text
6. **Trim** → leading/trailing whitespace stripped from transcription
7. **LLM format** (llm.rs) → optional cleanup via Chat Completions
8. **Prefix** → optional `paste_prefix` prepended (e.g. "🎙 ")
9. **History entry updated** in place with the result — before pasting, so text survives paste failures; on error → `failed` with error text
10. **Paste** (text.rs) → clipboard set → Cmd+V simulation (keycode 0x09, layout-independent)
11. **Frontend** listens to `recording-status-changed` / `history-changed` events, overlay shows status
12. **Cancel** → repeat hotkey during Processing aborts the pipeline (JoinHandle::abort); full cancel — the processing entry, WAV and meta are deleted

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
- **STT providers:** `stt_providers` is an array of `SttProvider` objects (name, base_url, model, api_key, request_format, extra_body). `active_stt_provider_index` selects which one to use. Three request formats: `"multipart"` (OpenAI-native form upload), `"json"` (OpenRouter-style JSON with base64 audio in `input_audio.data`), and `"gemini"` (Google generateContent: model in URL path, `x-goog-api-key` header, inline base64 WAV + transcribe instruction, `thinkingLevel: MINIMAL` by default). Extra body params are merged into the request (form fields for multipart, flattened JSON keys for json, root-level keys for gemini — a custom `generationConfig` replaces the default). Gemini base_url: `https://generativelanguage.googleapis.com/v1beta/models`; inline audio limit 20 MB ≈ 3.5 min of 44.1kHz mono WAV.
- **STT prompt:** Global `stt_prompt` setting sent with every STT request as context hint. Supported by OpenAI Whisper; may be ignored by other providers (e.g. Groq via OpenRouter).
- **Paste prefix:** Optional `paste_prefix` prepended to every transcription before pasting.
- **Pipeline cancellation:** Repeating the toggle hotkey during Processing state aborts the running async task via `JoinHandle::abort()`. State machine tracks Processing separately and resets to Idle on cancel or pipeline completion.
- **Re-transcription:** Every recording's WAV is saved to `last_recording.wav` (app data dir, single slot, overwritten by the next recording) before the STT call; `last_recording.json` links it to the history entry that owns it. The `retranscribe_last` command re-runs STT → LLM → prefix on the saved clip with *current* settings (so switching providers applies) and updates the owning entry in place — no Cmd+V paste (the app window is focused). Exposed in the history item context menu ("Re-transcribe"), shown only on the entry linked to the saved clip. During a re-transcribe the entry goes `processing` and the overlay shows the regular pipeline status events.
- **History entry statuses:** `ok` | `failed` | `processing`. The entry is created as `processing` the moment recording stops and is updated in place. A failed re-transcribe of an entry with good text keeps the text (status returns to `ok`, the error is stored as a hint shown under the text). Stale `processing` entries become `failed` ("Interrupted (app restarted)") on load. The paste-last hotkey pastes the newest `ok` entry, skipping failed/processing ones.

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
