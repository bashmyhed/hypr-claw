use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub llm_provider: LlmProvider,
    pub default_agent: String,
    pub sessions_dir: PathBuf,
    pub agents_dir: PathBuf,
    pub audit_log: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum LlmProvider {
    #[serde(rename = "nvidia_kiro")]
    NvidiaKiro { model: String },
    #[serde(rename = "openai")]
    OpenAI { model: String },
    #[serde(rename = "custom")]
    Custom { endpoint: String, model: String },
}

impl LlmProvider {
    pub fn endpoint(&self) -> String {
        std::env::var("HYPR_LLM_ENDPOINT").unwrap_or_else(|_| match self {
            LlmProvider::NvidiaKiro { .. } => {
                "https://integrate.api.nvidia.com/v1/chat/completions".to_string()
            }
            LlmProvider::OpenAI { .. } => {
                "https://api.openai.com/v1/chat/completions".to_string()
            }
            LlmProvider::Custom { endpoint, .. } => endpoint.clone(),
        })
    }

    pub fn model(&self) -> String {
        std::env::var("HYPR_LLM_MODEL").unwrap_or_else(|_| match self {
            LlmProvider::NvidiaKiro { model } => model.clone(),
            LlmProvider::OpenAI { model } => model.clone(),
            LlmProvider::Custom { model, .. } => model.clone(),
        })
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = PathBuf::from("config.toml");
        let content = std::fs::read_to_string(&config_path)
            .context("Failed to read config.toml")?;
        toml::from_str(&content).context("Failed to parse config.toml")
    }

    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write("config.toml", content)?;
        Ok(())
    }
}
