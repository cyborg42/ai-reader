use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub book_path: PathBuf,
    pub server_addr: String,
    pub log_file: Option<PathBuf>,
    pub storage_path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    api_key: String,
    base_url: String,
}

impl From<OpenAIConfig> for openai::Credentials {
    fn from(config: OpenAIConfig) -> Self {
        openai::Credentials::new(config.api_key, config.base_url)
    }
}
