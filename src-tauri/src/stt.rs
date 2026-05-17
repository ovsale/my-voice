//! Speech-to-text API client (OpenAI Whisper-compatible).
//!
//! Supports two request formats:
//! - **multipart**: Standard OpenAI Whisper format (multipart/form-data with file upload)
//! - **json**: OpenRouter-style format (application/json with base64-encoded audio)

use anyhow::{bail, Context};
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Default STT endpoint (OpenAI).
pub const DEFAULT_STT_BASE_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

/// Default STT model.
pub const DEFAULT_STT_MODEL: &str = "whisper-1";

/// Response from the Whisper-compatible transcription API.
#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
}

/// Error response from the API.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
}

/// Request format for the STT API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttRequestFormat {
    /// multipart/form-data (OpenAI native)
    Multipart,
    /// application/json with base64 audio (OpenRouter)
    Json,
}

impl SttRequestFormat {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            _ => Self::Multipart,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Multipart => "multipart",
            Self::Json => "json",
        }
    }
}

/// STT provider configuration.
pub struct SttConfig<'a> {
    pub base_url: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
    pub format: SttRequestFormat,
    /// Optional prompt/context hint for the STT model
    pub prompt: Option<&'a str>,
    /// Optional JSON string with extra body params, e.g. `{"provider":{"only":["groq"]}}`
    pub extra_body: Option<&'a str>,
}

/// JSON request body for OpenRouter-style APIs.
#[derive(Debug, Serialize)]
struct JsonTranscriptionRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    input_audio: InputAudio,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct InputAudio {
    data: String,
    format: String,
}

/// Transcribe audio using a Whisper-compatible API.
pub async fn transcribe(
    client: &reqwest::Client,
    config: &SttConfig<'_>,
    audio_wav: Vec<u8>,
    language: Option<&str>,
) -> anyhow::Result<String> {
    match config.format {
        SttRequestFormat::Multipart => transcribe_multipart(client, config, audio_wav, language).await,
        SttRequestFormat::Json => transcribe_json(client, config, audio_wav, language).await,
    }
}

/// Multipart format (OpenAI).
async fn transcribe_multipart(
    client: &reqwest::Client,
    config: &SttConfig<'_>,
    audio_wav: Vec<u8>,
    language: Option<&str>,
) -> anyhow::Result<String> {
    let file_part = reqwest::multipart::Part::bytes(audio_wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .context("Failed to set MIME type for audio part")?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", config.model.to_string());

    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }

    if let Some(prompt) = config.prompt {
        if !prompt.is_empty() {
            form = form.text("prompt", prompt.to_string());
        }
    }

    if let Some(extra) = config.extra_body {
        if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(extra) {
            for (key, value) in obj {
                let text_value = match &value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                form = form.text(key, text_value);
            }
        } else {
            log::warn!("STT extra_body is not a valid JSON object, ignoring: {extra}");
        }
    }

    let response = client
        .post(config.base_url)
        .bearer_auth(config.api_key)
        .multipart(form)
        .send()
        .await
        .context("Failed to send transcription request")?;

    parse_response(response).await
}

/// JSON format (OpenRouter).
async fn transcribe_json(
    client: &reqwest::Client,
    config: &SttConfig<'_>,
    audio_wav: Vec<u8>,
    language: Option<&str>,
) -> anyhow::Result<String> {
    let base64_audio = base64::engine::general_purpose::STANDARD.encode(&audio_wav);

    let extra = config
        .extra_body
        .and_then(|s| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(s).ok())
        .unwrap_or_default();

    let request_body = JsonTranscriptionRequest {
        model: config.model.to_string(),
        language: language.map(String::from),
        prompt: config.prompt.filter(|p| !p.is_empty()).map(String::from),
        input_audio: InputAudio {
            data: base64_audio,
            format: "wav".to_string(),
        },
        extra,
    };

    let response = client
        .post(config.base_url)
        .bearer_auth(config.api_key)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send transcription request")?;

    parse_response(response).await
}

/// Parse STT API response (shared between formats).
async fn parse_response(response: reqwest::Response) -> anyhow::Result<String> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response body>".to_string());

        if let Ok(error_response) = serde_json::from_str::<ApiErrorResponse>(&body) {
            bail!(
                "STT API error (HTTP {status}): {}",
                error_response.error.message
            );
        }

        bail!("STT API error (HTTP {status}): {body}");
    }

    let whisper_response: WhisperResponse = response
        .json()
        .await
        .context("Failed to parse STT API response JSON")?;

    Ok(whisper_response.text)
}
