//! OpenAI Chat Completions API client for text formatting.
//!
//! Sends raw transcription text to the Chat Completions endpoint with a
//! system prompt for cleanup/formatting, then returns the formatted text.

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

/// A single message in the chat completions request.
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

/// Request body for the Chat Completions API.
#[derive(Debug, Serialize)]
struct ChatCompletionsRequest {
    model: &'static str,
    messages: Vec<ChatMessage>,
}

/// A choice in the Chat Completions response.
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

/// The message content within a choice.
#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

/// Response body from the Chat Completions API.
#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
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

/// Format raw transcription text using the OpenAI Chat Completions API.
///
/// Sends the raw text as a user message along with a system prompt that instructs
/// the model how to format/clean up the transcription.
///
/// # Arguments
/// * `client` - A pre-configured `reqwest::Client` (allows connection pooling).
/// * `api_key` - OpenAI API key for Bearer token authentication.
/// * `system_prompt` - System message instructing the model on formatting rules.
/// * `raw_text` - The raw transcription text to format.
///
/// # Errors
/// Returns an error if the HTTP request fails, the API returns a non-success status,
/// the response contains no choices, or the response body cannot be parsed.
pub async fn format_text(
    client: &reqwest::Client,
    api_key: &str,
    system_prompt: &str,
    raw_text: &str,
) -> anyhow::Result<String> {
    let request_body = ChatCompletionsRequest {
        model: "gpt-4o-mini",
        messages: vec![
            ChatMessage {
                role: "system",
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user",
                content: raw_text.to_string(),
            },
        ],
    };

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request_body)
        .send()
        .await
        .context("Failed to send chat completions request to OpenAI")?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response body>".to_string());

        // Try to parse structured error
        if let Ok(error_response) = serde_json::from_str::<OpenAiErrorResponse>(&body) {
            bail!(
                "OpenAI Chat Completions API error (HTTP {status}): {}",
                error_response.error.message
            );
        }

        bail!("OpenAI Chat Completions API error (HTTP {status}): {body}");
    }

    let completions_response: ChatCompletionsResponse = response
        .json()
        .await
        .context("Failed to parse Chat Completions API response JSON")?;

    let content = completions_response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .context("Chat Completions API returned no choices or empty content")?;

    Ok(content)
}
