//! LLM client: provider presets, API-key storage in the OS keyring, and the
//! blueprint generation call. The model is asked to return ONLY JSON matching
//! the blueprint schema; the reply is strictly validated (unknown fields
//! dropped, doctrine checklist re-run by the caller).

use crate::blueprint::FamiliarBlueprint;
use crate::doctrine;
use crate::urlguard;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Nanogpt,
    Zai,
    #[serde(rename = "zai-coding")]
    ZaiCoding,
    Anthropic,
    Openai,
    Openrouter,
    Custom,
}

impl Provider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            // Endpoints mirrored from Proto-Familiar's PROVIDER_URLS —
            // Nano-GPT and z.ai are the standard providers there.
            Provider::Nanogpt => "https://nano-gpt.com/api/v1",
            Provider::Zai => "https://api.z.ai/api/paas/v4",
            Provider::ZaiCoding => "https://api.z.ai/api/coding/paas/v4",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Openai => "https://api.openai.com/v1",
            Provider::Openrouter => "https://openrouter.ai/api/v1",
            Provider::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Nanogpt => "gpt-4o-mini",
            Provider::Zai | Provider::ZaiCoding => "glm-4.7",
            Provider::Anthropic => "claude-sonnet-4-5",
            Provider::Openai => "gpt-4.1",
            Provider::Openrouter => "anthropic/claude-sonnet-4.5",
            Provider::Custom => "",
        }
    }
}

/// Non-secret provider configuration (stored as JSON in the app config dir).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: Provider,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub allow_local: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Anthropic,
            base_url: Provider::Anthropic.default_base_url().into(),
            model: Provider::Anthropic.default_model().into(),
            allow_local: false,
        }
    }
}

const KEYRING_SERVICE: &str = "advoco";

fn keyring_account(cfg: &LlmConfig) -> String {
    match cfg.provider {
        Provider::Custom => format!("custom:{}", cfg.base_url),
        p => p_label(p).to_string(),
    }
}

fn p_label(p: Provider) -> &'static str {
    match p {
        Provider::Nanogpt => "nanogpt",
        Provider::Zai => "zai",
        Provider::ZaiCoding => "zai-coding",
        Provider::Anthropic => "anthropic",
        Provider::Openai => "openai",
        Provider::Openrouter => "openrouter",
        Provider::Custom => "custom",
    }
}

pub fn store_api_key(cfg: &LlmConfig, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(cfg))
        .map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn load_api_key(cfg: &LlmConfig) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(cfg))
        .map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("No API key stored yet — add one on the connection screen.")]
    NoKey,
    #[error("{0}")]
    BadUrl(String),
    #[error("The model provider said no: {0}")]
    Http(String),
    #[error("The model's reply was not valid JSON: {0}")]
    Parse(String),
    #[error("The model's reply failed the doctrine checklist.")]
    Doctrine,
}

/// One chat completion: messages in, raw text out. Anthropic uses /messages;
/// everything else uses the OpenAI-compatible /chat/completions shape.
async fn complete(
    cfg: &LlmConfig,
    api_key: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, LlmError> {
    let base = if cfg.base_url.is_empty() {
        cfg.provider.default_base_url()
    } else {
        cfg.base_url.trim_end_matches('/')
    };
    urlguard::validate_llm_url(base, cfg.allow_local)
        .map_err(|e| LlmError::BadUrl(e.to_string()))?;

    let client = reqwest::Client::new();
    match cfg.provider {
        Provider::Anthropic => {
            let url = format!("{base}/messages");
            let body = serde_json::json!({
                "model": cfg.model,
                "max_tokens": max_tokens,
                "system": system,
                "messages": [{ "role": "user", "content": user }],
            });
            let resp = client
                .post(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let status = resp.status();
            let json: Value = resp
                .json()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            if !status.is_success() {
                return Err(LlmError::Http(format!(
                    "{} {}",
                    status,
                    json.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                )));
            }
            let text = json
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| {
                            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                                b.get("text")
                                    .and_then(|t| t.as_str())
                                    .map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect::<String>()
                })
                .unwrap_or_default();
            Ok(text)
        }
        Provider::Openai | Provider::Openrouter | Provider::Nanogpt
        | Provider::Zai | Provider::ZaiCoding | Provider::Custom => {
            let url = format!("{base}/chat/completions");
            let mut req = client
                .post(&url)
                .bearer_auth(api_key)
                .json(&serde_json::json!({
                    "model": cfg.model,
                    "max_tokens": max_tokens,
                    "messages": [
                        { "role": "system", "content": system },
                        { "role": "user", "content": user },
                    ],
                }));
            if cfg.provider == Provider::Openrouter {
                req = req
                    .header("HTTP-Referer", "https://github.com/ActualBroeckchen/Advoco")
                    .header("X-Title", "Advoco");
            }
            let resp = req
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let status = resp.status();
            let json: Value = resp
                .json()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            if !status.is_success() {
                return Err(LlmError::Http(format!(
                    "{} {}",
                    status,
                    json.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                )));
            }
            let text = json
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            Ok(text)
        }
    }
}

/// Cheap connectivity + auth probe.
pub async fn test_connection(cfg: &LlmConfig) -> Result<(), LlmError> {
    let key = load_api_key(cfg)
        .ok()
        .flatten()
        .ok_or(LlmError::NoKey)?;
    let reply = complete(
        cfg,
        &key,
        "You are a connection test. Reply with the single word: ready.",
        "Ping.",
        16,
    )
    .await?;
    let _ = reply;
    Ok(())
}

/// Wizard inputs for one generation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerationInput {
    /// The ward's free-text description ("a haughty cat").
    pub concept: String,
    /// Optional chosen name.
    pub name: String,
    /// firm | gentle | competitive | adaptive | discover
    pub support_stance: String,
    /// Optional chatlog excerpts the ward dropped in (already trimmed to a
    /// manageable size by the UI).
    pub chatlog_excerpts: String,
    /// Optional answers to the sketch questions.
    pub followup_answers: Vec<String>,
}

/// Extract the JSON object from a reply that may be fenced or prefixed.
fn extract_json(text: &str) -> Result<serde_json::Value, LlmError> {
    let trimmed = text.trim();
    let candidate = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };
    serde_json::from_str(candidate).map_err(|e| LlmError::Parse(e.to_string()))
}

pub async fn generate_blueprint(
    cfg: &LlmConfig,
    input: &GenerationInput,
) -> Result<FamiliarBlueprint, LlmError> {
    let key = load_api_key(cfg)
        .ok()
        .flatten()
        .ok_or(LlmError::NoKey)?;

    let system = format!(
        "{}\n\n{}\n\n{}\n\nYou return ONLY a single JSON object, no prose, no code fences, matching this TypeScript shape:\n{}",
        doctrine::generation_doctrine(),
        doctrine::support_stance_doctrine(&input.support_stance),
        doctrine::posthistory_shape(),
        SCHEMA_HINT,
    );

    let mut user = String::new();
    user.push_str(&format!(
        "The ward described their future familiar as: \"{}\"\n",
        input.concept
    ));
    if !input.name.is_empty() {
        user.push_str(&format!("Chosen name: {}\n", input.name));
    }
    user.push_str(&format!(
        "Support stance chosen by the ward: {}\n",
        input.support_stance
    ));
    if !input.followup_answers.is_empty() {
        user.push_str("\nAdditional answers from the ward:\n");
        for a in &input.followup_answers {
            user.push_str(&format!("- {a}\n"));
        }
    }
    if !input.chatlog_excerpts.is_empty() {
        user.push_str(
            "\nExcerpts of the ward's past conversations with another AI companion follow. \
Distill the ward's user_facts and graph_entities from them; if the old companion had a \
personality the ward clearly enjoyed, take cues for traits/voice (but this familiar is a \
new being, not that companion resurrected). Never invent facts that are not supported.\n\n\
---BEGIN EXCERPTS---\n",
        );
        user.push_str(&input.chatlog_excerpts);
        user.push_str("\n---END EXCERPTS---\n");
    }

    // Two attempts: the second feeds back validation errors.
    let mut last_err = None;
    for attempt in 0..2 {
        let prompt = if attempt == 0 {
            user.clone()
        } else {
            format!(
                "{}\n\nYour previous reply was rejected: {}. Return the corrected JSON object only.",
                user,
                last_err.clone().unwrap_or_else(|| "invalid reply".into())
            )
        };
        let reply = complete(cfg, &key, &system, &prompt, 4096).await?;
        let value = match extract_json(&reply) {
            Ok(v) => v,
            Err(e) => {
                last_err = Some(e.to_string());
                continue;
            }
        };
        // Strict-ish: unknown fields dropped, missing fields defaulted.
        let bp: FamiliarBlueprint = match serde_json::from_value(value) {
            Ok(bp) => bp,
            Err(e) => {
                last_err = Some(format!("schema mismatch: {e}"));
                continue;
            }
        };
        return Ok(bp);
    }
    Err(LlmError::Parse(last_err.unwrap_or_else(|| "empty reply".into())))
}

const SCHEMA_HINT: &str = r#"{
  name: string,                    // if the ward didn't pick one, invent a fitting one
  species: string,                 // the animal or mythological beast, e.g. "cat", "wyvern"
  concept: string,                 // the ward's own words, echoed
  relationshipArchetype: string,   // e.g. "a working dog to its handler"
  supportStance: string,           // echo the ward's stance key
  traits: string[],                // each phrased as observable behavior, negative traits amplified
  backstory: string,               // 2-4 sentences, structural roleplay roots
  wants: string[],
  boundaries: string[],
  bodyLanguage: string[],          // 4-7 species-specific behaviors
  warmthExpression: string[],      // warmth without romance, concrete
  textureAnchors: string[],        // 2-4 anchors: Myers-Briggs, Enneagram wing,
                                   //   TVTropes names — always include a
                                   //   Bond-Animal-adjacent trope if it fits
  exampleDialogues: [              // exactly 3, in this order, each written in
                                    // THIS character's unmistakable voice
    { scenario: "flirt_deflection", userLine: string, familiarLine: string },
    { scenario: "excuse_callout", userLine: string, familiarLine: string },
    { scenario: "gruff_warmth", userLine: string, familiarLine: string }
  ],
  voice: { description: string,
           register: string,       // one dense character-card register line
           dialect: string, accent: string,
           tics: string[], signaturePhrases: string[],
           styleReferences: string[] }, // 4-6 quoted lines this Familiar
                                        //   would actually say, each doing
                                        //   character work
  userFacts: string[],             // about the ward, only from excerpts; empty if none
  graphEntities: [ { label: string, type: "person"|"pet"|"project"|"place"|"organisation"|"thing",
                     relation: string, description: string } ],
  reinforcement: string            // ~60 words: form & bond line, then voice line
}"#;
