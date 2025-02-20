use std::sync::OnceLock;

use anyhow::bail;

pub static OPENAI_API_KEY: OnceLock<openai::Credentials> = OnceLock::new();

pub async fn summarize(content: &str, limit: usize) -> anyhow::Result<String> {
    if limit < 10 {
        bail!("limit must be greater than 10");
    }
    let credentials = OPENAI_API_KEY
        .get()
        .ok_or(anyhow::anyhow!("OPENAI_API_KEY is not set"))?
        .clone();
    let prompt = format!(
        "Provide a concise summary of the following text in {} words or less. Return only the summary without any additional text or explanation:\n{}",
        limit, content
    );
    let summary = openai::chat::ChatCompletion::builder(
        "gpt-4o-mini",
        vec![openai::chat::ChatCompletionMessage {
            role: openai::chat::ChatCompletionMessageRole::User,
            content: Some(prompt),
            ..Default::default()
        }],
    )
    .credentials(credentials)
    .create()
    .await?
    .choices[0]
        .message
        .content
        .clone()
        .ok_or(anyhow::anyhow!("No response from OpenAI"))?;
    if summary.split_whitespace().count() > limit {
        bail!("summary is too long");
    }
    Ok(summary)
}
