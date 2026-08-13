//! Speech-to-text API client.
//!
//! Supports three request formats:
//! - **multipart**: Standard OpenAI Whisper format (multipart/form-data with file upload)
//! - **json**: OpenRouter-style format (application/json with base64-encoded audio)
//! - **gemini**: Google Gemini `generateContent` (audio understanding with a transcribe prompt)

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
    /// Gemini generateContent with inline audio (Google AI Studio)
    Gemini,
}

impl SttRequestFormat {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            "gemini" => Self::Gemini,
            _ => Self::Multipart,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Multipart => "multipart",
            Self::Json => "json",
            Self::Gemini => "gemini",
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
        SttRequestFormat::Gemini => transcribe_gemini(client, config, audio_wav, language).await,
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

/// Gemini generateContent format (Google AI Studio).
///
/// The model is addressed in the URL path (`{base_url}/{model}:generateContent`),
/// auth goes into the `x-goog-api-key` header, and the audio is sent inline as
/// base64 WAV together with a transcription instruction. `thinkingLevel: MINIMAL`
/// is set by default — without it flash-lite burns ~400 thinking tokens per clip.
/// `extra_body` keys are merged into the request root (e.g. a custom
/// `generationConfig` replaces the default one).
async fn transcribe_gemini(
    client: &reqwest::Client,
    config: &SttConfig<'_>,
    audio_wav: Vec<u8>,
    language: Option<&str>,
) -> anyhow::Result<String> {
    let base64_audio = base64::engine::general_purpose::STANDARD.encode(&audio_wav);

    let mut instruction = String::from(
        "Transcribe this audio verbatim. Output only the transcription text, nothing else.",
    );
    if let Some(lang) = language {
        instruction.push_str(&format!(" The audio language is {lang}."));
    }
    if let Some(prompt) = config.prompt {
        if !prompt.is_empty() {
            instruction.push_str("\n\nContext: ");
            instruction.push_str(prompt);
        }
    }

    let mut body = serde_json::json!({
        "contents": [{
            "parts": [
                { "text": instruction },
                { "inline_data": { "mime_type": "audio/wav", "data": base64_audio } },
            ],
        }],
        "generationConfig": { "thinkingConfig": { "thinkingLevel": "MINIMAL" } },
    });

    if let Some(extra) = config.extra_body {
        if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(extra) {
            if let Some(body_object) = body.as_object_mut() {
                for (key, value) in obj {
                    body_object.insert(key, value);
                }
            }
        } else {
            log::warn!("STT extra_body is not a valid JSON object, ignoring: {extra}");
        }
    }

    let url = format!(
        "{}/{}:generateContent",
        config.base_url.trim_end_matches('/'),
        config.model
    );

    let response = client
        .post(&url)
        .header("x-goog-api-key", config.api_key)
        .json(&body)
        .send()
        .await
        .context("Failed to send transcription request")?;

    parse_gemini_response(response).await
}

/// Response from the Gemini generateContent API.
#[derive(Debug, Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

/// Parse a Gemini generateContent response into the transcription text.
async fn parse_gemini_response(response: reqwest::Response) -> anyhow::Result<String> {
    let response = fail_on_error_status(response).await?;

    let gemini_response: GeminiResponse = response
        .json()
        .await
        .context("Failed to parse Gemini API response JSON")?;

    let text: String = gemini_response
        .candidates
        .first()
        .and_then(|candidate| candidate.content.as_ref())
        .map(|content| {
            content
                .parts
                .iter()
                .filter_map(|part| part.text.as_deref())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    if text.is_empty() {
        bail!("Gemini API returned no transcription text (empty or blocked response)");
    }

    Ok(text)
}

/// Parse STT API response (multipart and json formats).
async fn parse_response(response: reqwest::Response) -> anyhow::Result<String> {
    let response = fail_on_error_status(response).await?;

    let whisper_response: WhisperResponse = response
        .json()
        .await
        .context("Failed to parse STT API response JSON")?;

    Ok(whisper_response.text)
}

/// Bail with a readable message if the response status is not a success.
/// The `{"error": {"message": ...}}` shape covers OpenAI, OpenRouter and Gemini.
async fn fail_on_error_status(response: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

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
