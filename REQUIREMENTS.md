# Voice Dictation App — Requirements

## Goal

Minimal voice dictation app for macOS. Record voice via hotkey, transcribe via cloud API, insert text into the active window. No Python, no pipecat, no WebRTC. Everything in Rust + TypeScript/React (Tauri).

## Core Flow

```
Hotkey press → Mic capture → Audio buffer → STT API (HTTP) → Text → Paste into active app
                                                   ↓ (optional)
                                              LLM API (HTTP) → Formatted text
```

## Functional Requirements

### 1. Recording

- Global hotkey triggers start/stop (toggle mode) or hold-to-record
- Native mic capture via `cpal` (low latency, bypass browser overhead)
- Audio buffered in memory during recording
- On stop: encode to format accepted by STT API (e.g. WAV, WebM/Opus)

### 2. Transcription (STT)

- Send recorded audio to cloud STT API via HTTP POST
- MVP: OpenAI Whisper API (`/v1/audio/transcriptions`)
- Later: support switching between providers (Deepgram, Groq, etc.)
- API key stored in local settings

### 3. Text Formatting (Optional LLM)

- If enabled: send raw transcription to LLM API for cleanup/formatting
- MVP: OpenAI Chat Completions API
- Configurable system prompt
- Can be disabled (raw transcription used directly)

### 4. Text Insertion

- Paste result into the active app via clipboard + Cmd+V simulation
- Save/restore clipboard content around paste
- Use `enigo` crate for keyboard simulation

### 5. Overlay Window

- Floating 48x48 transparent always-on-top button
- Shows recording state (idle / recording / processing / error)
- Draggable
- Visible on all Spaces/desktops
- macOS: NSPanel with ScreenSaver level (above fullscreen apps)

### 6. Settings

- Server URL not needed (direct API calls)
- API keys (OpenAI, etc.) — stored locally in Tauri store
- Hotkey configuration (toggle, hold, paste-last)
- Mic device selection
- Sound effects on/off
- LLM formatting on/off
- System prompt customization

### 7. History

- Store transcriptions in local SQLite
- Show history in main window with date grouping
- "Paste last" hotkey to re-insert previous transcription

## Non-Functional Requirements

- macOS 12+ support (no onnxruntime, no opencv, no Python)
- Single binary (Tauri bundle)
- Minimal dependencies
- Fast startup

## Architecture

```
┌─────────────────────────────────────────┐
│  Tauri App                              │
│                                         │
│  ┌──────────┐    ┌───────────────────┐  │
│  │ Overlay   │    │ Main Window       │  │
│  │ (React)   │    │ (React)           │  │
│  │ - Mic btn │    │ - Settings        │  │
│  │ - Status  │    │ - History         │  │
│  └────┬──────┘    └───────────────────┘  │
│       │                                  │
│  ┌────▼──────────────────────────────┐   │
│  │ Rust Backend                      │   │
│  │ - Mic capture (cpal)              │   │
│  │ - Audio encoding                  │   │
│  │ - HTTP client (reqwest)           │   │
│  │   → STT API                       │   │
│  │   → LLM API                       │   │
│  │ - Text insertion (enigo)          │   │
│  │ - Hotkey handling                 │   │
│  │ - Settings (tauri-store)          │   │
│  │ - History (SQLite)                │   │
│  │ - Active app context              │   │
│  └───────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

Key difference from Tambourine: **no server process, no WebRTC**. Rust backend calls STT/LLM APIs directly via HTTP.

## What to Reuse from Tambourine

### Take as-is (or with minor adaptation)

| What | Source | Notes |
|------|--------|-------|
| Text insertion | `app/src-tauri/src/commands/text.rs` | Clipboard + Cmd+V, works perfectly |
| Active app context (macOS) | `app/src-tauri/src/active_app_context/macos.rs` | Accessibility API watcher |
| Active app context (Windows) | `app/src-tauri/src/active_app_context/windows.rs` | UI Automation API |
| Focus watcher | `app/src-tauri/src/active_app_context/watcher.rs` | Background polling thread |
| Audio mute | `app/src-tauri/src/audio_mute/` | CoreAudio/WASAPI volume toggle |
| Sound effects | `app/src-tauri/src/audio.rs` | Recording start/stop beeps |
| Hotkey state machine | `app/src-tauri/src/lib.rs` lines 250-340 | `ShortcutState` enum + transitions |
| Hotkey defaults | `app/src/lib/hotkeyDefaults.ts` | Default key combos |
| Settings schema | `app/src-tauri/src/settings.rs` | Setting types and persistence |
| Settings commands | `app/src-tauri/src/commands/settings.rs` | Tauri IPC for settings CRUD |
| History DB | `app/src-tauri/src/history.rs` | SQLite storage |
| History commands | `app/src-tauri/src/commands/history.rs` | Tauri IPC for history |
| History UI | `app/src/components/HistoryFeed.tsx` | Date-grouped feed |
| Overlay window setup | `app/src-tauri/src/lib.rs` lines 559-600 | NSPanel, always-on-top, all Spaces |
| Overlay resize | `app/src-tauri/src/commands/overlay.rs` | Content-driven resize |
| Export/import | `app/src-tauri/src/commands/export_import.rs` | Settings/history export |
| Mic capture | `app/src-tauri/src/mic_capture/cpal_impl.rs` | cpal-based native capture |
| Events | `app/src-tauri/src/events.rs` | Type-safe Tauri events |
| App state | `app/src-tauri/src/state.rs` | Shared app state |

### Take UX, rewrite logic

| What | Source | Notes |
|------|--------|-------|
| Overlay UI | `app/src/OverlayApp.tsx` | Keep visual states, remove WebRTC/Pipecat logic |
| Settings UI | `app/src/components/settings/` | Remove provider switching complexity, keep layout |
| Main window | `app/src/App.tsx` | Keep sidebar + nav, simplify |
| Hotkey settings | `app/src/components/settings/HotkeySettings.tsx` | Reuse as-is |
| Audio settings | `app/src/components/settings/AudioSettings.tsx` | Keep mic selection, remove server-dependent parts |
| Prompt settings | `app/src/components/settings/PromptSettings.tsx` | Simplify for local-only prompts |
| Tauri IPC types | `app/src/lib/tauri.ts` | Keep type definitions, remove server/WebRTC calls |
| Recording store | `app/src/stores/recordingStore.ts` | Simplify state (no connection machine needed) |

### Do NOT take

| What | Why |
|------|-----|
| `app/src/machines/connectionMachine.ts` | XState machine for WebRTC — not needed |
| `app/src/lib/safeSendClientMessage.ts` | WebRTC message wrapper — not needed |
| `app/src/lib/nativeAudio.ts` | Streams audio to WebRTC — new approach: buffer in Rust |
| `app/src/hooks/useNativeAudioTrack.ts` | WebRTC audio track — not needed |
| `public/native-audio-processor.js` | AudioWorklet for WebRTC — not needed |
| `app/src/lib/queries.ts` | React Query for server sync — not needed |
| `app/src/contexts/ConnectionContext.tsx` | WebRTC connection context — not needed |
| `app/src/components/settings/ConnectionSettings.tsx` | Server URL config — not needed |
| `app/src/components/settings/ProvidersSettings.tsx` | Complex provider switching — simplify to API key input |
| Entire `server/` directory | Python server — the whole point is to remove this |

## New Code to Write

### Rust

1. **Audio encoder** — convert PCM from cpal to WAV or Opus in memory
2. **STT HTTP client** — `reqwest` multipart POST to OpenAI `/v1/audio/transcriptions`
3. **LLM HTTP client** — `reqwest` POST to OpenAI `/v1/chat/completions`
4. **Recording orchestrator** — coordinates: capture → encode → transcribe → (format) → paste
5. **Tauri commands** for new flow: `start_recording`, `stop_recording`, `get_transcription_status`

### TypeScript

1. **Simplified overlay state** — just idle/recording/processing/done/error (no connection states)
2. **API key settings page** — simple input fields instead of provider dropdowns
3. **Remove all WebRTC/Pipecat/server code**

## Audio Encoding Notes

OpenAI Whisper API accepts: flac, mp3, mp4, mpeg, mpga, m4a, ogg, wav, webm.
Simplest: WAV (just a header + raw PCM). No external encoder needed.

Rust crates:
- `hound` — WAV encoding (tiny, pure Rust)
- `cpal` — already used for capture
- `reqwest` — HTTP client with multipart support

## MVP Scope

1. Toggle hotkey → record → stop → OpenAI Whisper API → paste text
2. Overlay button with state indication
3. API key in settings
4. That's it. Everything else is iteration.
