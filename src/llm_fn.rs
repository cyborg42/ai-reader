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

#[cfg(test)]
mod tests {
    use crate::config::OpenAIConfig;

    use super::*;

    #[test]
    fn test_summarize() {
        let key = std::fs::read_to_string("./openai_api_key.toml").unwrap();
        let key: openai::Credentials = toml::from_str::<OpenAIConfig>(&key).unwrap().into();
        OPENAI_API_KEY.set(key).unwrap();
        let story = "Once upon a time, there was a young programmer who loved to code. Every day, she would spend hours crafting elegant solutions to complex problems. Her passion for programming grew stronger with each line of code she wrote. One day, she created an amazing application that helped many people. The joy of seeing others benefit from her work made all the late nights worth it. She realized that programming wasn't just about writing code - it was about making a difference in the world.";
        let summary = summarize(story, 20);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(summary);
        let summary = result.unwrap();
        println!("{}", summary);
    }
}
