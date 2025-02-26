use std::sync::LazyLock;

use crate::teacher::messages::BookProgress;
use anyhow::bail;
use openai::chat::ChatCompletionMessage;
use schemars::SchemaGenerator;

pub static OPENAI_API_KEY: LazyLock<openai::Credentials> = LazyLock::new(|| {
    let _ = dotenvy::dotenv();
    let key = dotenvy::var("OPENAI_KEY").unwrap();
    let base_url = dotenvy::var("OPENAI_BASE_URL").unwrap();
    openai::Credentials::new(key, base_url)
});

pub static AI_MODEL: LazyLock<String> = LazyLock::new(|| {
    let _ = dotenvy::dotenv();
    dotenvy::var("AI_MODEL").unwrap()
});

pub fn get_json_generator() -> SchemaGenerator {
    let mut settings = schemars::r#gen::SchemaSettings::default();
    settings.option_add_null_type = false;
    settings.option_nullable = false;
    SchemaGenerator::new(settings)
}

pub async fn summarize(content: &str, limit: usize) -> anyhow::Result<String> {
    if limit < 10 {
        bail!("limit must be greater than 10");
    }
    let credentials = OPENAI_API_KEY.clone();
    let prompt = format!(
        "Provide a concise summary of the following text in {} words or less. Return only the summary without any additional text or explanation:\n{}",
        limit, content
    );
    let choises = openai::chat::ChatCompletion::builder(
        AI_MODEL.as_str(),
        vec![ChatCompletionMessage {
            role: openai::chat::ChatCompletionMessageRole::User,
            content: Some(prompt),
            ..Default::default()
        }],
    )
    .credentials(credentials)
    .create()
    .await?;
    let summary = choises.choices[0]
        .message
        .content
        .clone()
        .ok_or(anyhow::anyhow!("No response from OpenAI"))?;
    Ok(summary)
}

pub async fn summarize_progress(
    messages: Vec<ChatCompletionMessage>,
    limit: usize,
) -> anyhow::Result<BookProgress> {
    todo!()
}

pub fn token_count(content: &str) -> usize {
    content.len() / 4
}
pub fn message_token_count(message: &ChatCompletionMessage) -> usize {
    message
        .content
        .as_ref()
        .map_or(0, |content| token_count(content))
}
