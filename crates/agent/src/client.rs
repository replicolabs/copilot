use serde::{Deserialize, Serialize};

use crate::Error;

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    temperature: f32,
    system: &'a str,
    messages: [RequestMessage<'a>; 1],
}

#[derive(Serialize)]
struct RequestMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicClient {
    pub fn from_env() -> Result<Self, Error> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or(Error::MissingCredential)?;
        Ok(Self::new(api_key, DEFAULT_MODEL.to_owned()))
    }

    pub fn new(api_key: String, model: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            model,
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub async fn complete(&self, system: &str, user: &str) -> Result<String, Error> {
        let body = MessagesRequest {
            model: &self.model,
            max_tokens: MAX_TOKENS,
            temperature: 0.0,
            system,
            messages: [RequestMessage {
                role: "user",
                content: user,
            }],
        };

        let response = self
            .http
            .post(MESSAGES_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                detail,
            });
        }

        let parsed: MessagesResponse = response.json().await?;
        let text: String = parsed
            .content
            .into_iter()
            .filter(|block| block.kind == "text")
            .map(|block| block.text)
            .collect();

        if text.trim().is_empty() {
            return Err(Error::EmptyResponse);
        }
        Ok(text)
    }
}
