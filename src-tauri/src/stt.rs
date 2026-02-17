//! OpenAI Whisper API client for speech-to-text transcription.
//!
//! Sends WAV audio data to the OpenAI `/v1/audio/transcriptions` endpoint
//! and returns the transcribed text.

use anyhow::{bail, Context};
use serde::Deserialize;

/// Response from the OpenAI Whisper transcription API.
#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
}

/// Error response from the OpenAI API.
#[derive(Debug, Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorDetail {
    message: String,
}

/// Transcribe audio using the OpenAI Whisper API.
///
/// Sends a WAV audio buffer to the `/v1/audio/transcriptions` endpoint using
/// multipart/form-data. Returns the transcribed text on success.
///
/// # Arguments
/// * `client` - A pre-configured `reqwest::Client` (allows connection pooling).
/// * `api_key` - OpenAI API key for Bearer token authentication.
/// * `audio_wav` - WAV-encoded audio data as a byte vector.
/// * `language` - Optional BCP-47 language code (e.g., "en", "ja") to hint the model.
///
/// # Errors
/// Returns an error if the HTTP request fails, the API returns a non-success status,
/// or the response body cannot be parsed.
pub async fn transcribe(
    client: &reqwest::Client,
    api_key: &str,
    audio_wav: Vec<u8>,
    language: Option<&str>,
) -> anyhow::Result<String> {
    let file_part = reqwest::multipart::Part::bytes(audio_wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .context("Failed to set MIME type for audio part")?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", "whisper-1");

    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .context("Failed to send transcription request to OpenAI")?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response body>".to_string());

        // Try to parse structured error
        if let Ok(error_response) = serde_json::from_str::<OpenAiErrorResponse>(&body) {
            bail!(
                "OpenAI Whisper API error (HTTP {status}): {}",
                error_response.error.message
            );
        }

        bail!("OpenAI Whisper API error (HTTP {status}): {body}");
    }

    let whisper_response: WhisperResponse = response
        .json()
        .await
        .context("Failed to parse Whisper API response JSON")?;

    Ok(whisper_response.text)
}
