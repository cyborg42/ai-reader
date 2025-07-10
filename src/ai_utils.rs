use std::sync::LazyLock;

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionNamedToolChoice, ChatCompletionRequestMessage, ChatCompletionTool,
        ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionName, FunctionObject,
    },
};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub static AI_MODEL: LazyLock<String> = LazyLock::new(|| dotenvy::var("AI_MODEL").unwrap());

pub static AI_CLIENT: LazyLock<Client<OpenAIConfig>> = LazyLock::new(|| {
    let api_key = dotenvy::var("OPENAI_API_KEY").unwrap();
    let base_url = dotenvy::var("OPENAI_BASE_URL").unwrap();
    let config = OpenAIConfig::default()
        .with_api_base(base_url)
        .with_api_key(api_key);
    Client::with_config(config)
});

pub trait Tokens {
    fn tokens(&self) -> u64;
}
impl Tokens for String {
    fn tokens(&self) -> u64 {
        (self.len() + 2) as u64 / 4
    }
}
impl Tokens for str {
    fn tokens(&self) -> u64 {
        (self.len() + 2) as u64 / 4
    }
}
impl Tokens for ChatCompletionRequestMessage {
    fn tokens(&self) -> u64 {
        match self {
            ChatCompletionRequestMessage::System(content) => {
                    match &content.content {
                        async_openai::types::ChatCompletionRequestSystemMessageContent::Text(text) => text.tokens(),
                        async_openai::types::ChatCompletionRequestSystemMessageContent::Array(parts) => parts.iter().map(|p| match p{
                            async_openai::types::ChatCompletionRequestSystemMessageContentPart::Text(text) => text.text.tokens(),
                        }).sum(),
                    }
                },
            ChatCompletionRequestMessage::User(content) => {
                match &content.content {
                    async_openai::types::ChatCompletionRequestUserMessageContent::Text(text) => text.tokens(),
                    async_openai::types::ChatCompletionRequestUserMessageContent::Array(parts) => parts.iter().map(|p| match p{
                        async_openai::types::ChatCompletionRequestUserMessageContentPart::Text(text) => text.text.tokens(),
                        async_openai::types::ChatCompletionRequestUserMessageContentPart::ImageUrl(_image) => 170*4,
                        async_openai::types::ChatCompletionRequestUserMessageContentPart::InputAudio(audio) => audio.input_audio.data.tokens(),
                    }).sum(),
                }
            },
            ChatCompletionRequestMessage::Assistant(content) => {
                match &content.content {
                    Some(async_openai::types::ChatCompletionRequestAssistantMessageContent::Text(text)) => text.tokens(),
                    Some(async_openai::types::ChatCompletionRequestAssistantMessageContent::Array(parts)) => parts.iter().map(|p| match p{
                        async_openai::types::ChatCompletionRequestAssistantMessageContentPart::Text(text) => text.text.tokens(),
                        async_openai::types::ChatCompletionRequestAssistantMessageContentPart::Refusal(refusal) => refusal.refusal.tokens(),
                    }).sum(),
                    None => 0,
                }
            },
            ChatCompletionRequestMessage::Tool(content) => {
                match &content.content {
                    async_openai::types::ChatCompletionRequestToolMessageContent::Text(text) => text.tokens(),
                    async_openai::types::ChatCompletionRequestToolMessageContent::Array(parts) => parts.iter().map(|p| match p{
                        async_openai::types::ChatCompletionRequestToolMessageContentPart::Text(text) => text.text.tokens(),
                    }).sum(),
                }
            },
            ChatCompletionRequestMessage::Function(content) => {
                content.content.as_ref().map_or(0, |text| text.tokens())
            },
            ChatCompletionRequestMessage::Developer(content) => {
                match &content.content {
                    async_openai::types::ChatCompletionRequestDeveloperMessageContent::Text(text) => text.tokens(),
                    async_openai::types::ChatCompletionRequestDeveloperMessageContent::Array(parts) => parts.iter().map(|p| p.text.tokens()).sum(),
                }
            },
        }
    }
}

pub async fn summarize(
    content: &str,
    limit: usize,
    prompt: Option<String>,
) -> anyhow::Result<String> {
    let prompt = match prompt {
        Some(prompt) => format!(
            "{prompt}\n\nProvide a concise result of the following text in {} words or less. Return only the result without any additional text or explanation:\n{}",
            limit, content
        ),
        None => format!(
            "Provide a concise summary of the following text in {} words or less. Return only the summary without any additional text or explanation:\n{}",
            limit, content
        ),
    };
    let request = CreateChatCompletionRequestArgs::default()
        .model(AI_MODEL.as_str())
        .messages(vec![ChatCompletionRequestMessage::User(prompt.into())])
        .build()
        .unwrap();
    let response = AI_CLIENT.chat().create(request).await?;
    let summary = response
        .choices
        .first()
        .ok_or(anyhow::anyhow!("No response from OpenAI"))?
        .message
        .content
        .clone()
        .ok_or(anyhow::anyhow!("No response from OpenAI"))?;
    Ok(summary)
}

pub async fn extract_key_points(content: &str) -> anyhow::Result<Vec<String>> {
    #[derive(Debug, JsonSchema, Serialize, Deserialize)]
    struct KeyPoints(Vec<String>);
    let tool = extract_tool::<KeyPoints>(None);
    let tool_choice = ChatCompletionToolChoiceOption::Named(ChatCompletionNamedToolChoice {
        r#type: ChatCompletionToolType::Function,
        function: FunctionName {
            name: tool.function.name.clone(),
        },
    });
    let prompt = format!(
        "Extract the key points from the following text:\n{}",
        content
    );
    let request = CreateChatCompletionRequestArgs::default()
        .model(AI_MODEL.as_str())
        .messages(vec![ChatCompletionRequestMessage::User(prompt.into())])
        .tools(vec![tool])
        .tool_choice(tool_choice)
        .build()
        .unwrap();
    let response = AI_CLIENT
        .chat()
        .create(request)
        .await?
        .choices
        .first()
        .ok_or(anyhow::anyhow!("No response from OpenAI"))?
        .message
        .tool_calls
        .as_ref()
        .and_then(|tool_calls| tool_calls.first())
        .ok_or(anyhow::anyhow!("No tool call in response"))?
        .function
        .arguments
        .clone();
    let key_points: KeyPoints = serde_json::from_str(&response)?;
    Ok(key_points.0)
}

pub fn extract_tool<T: JsonSchema>(strict: Option<bool>) -> ChatCompletionTool {
    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: T::schema_name(),
            description: None,
            parameters: Some(json!(schema_for!(T))),
            strict,
        },
    }
}

#[cfg(test)]
mod tests {

    use std::collections::BTreeMap;

    use async_openai::{tools::Tool, tools::ToolManager, types::ChatCompletionMessageToolCall};

    use super::*;
    use crate::{
        books::chapter::{ChapterNumber, ChapterRaw},
        utils::init_log,
    };

    #[tokio::test]
    async fn test_tool_manager() {
        let _guard = init_log(None);
        struct QueryChapterTool {
            chapters: BTreeMap<ChapterNumber, ChapterRaw>,
        }
        impl Tool for QueryChapterTool {
            type Args = ChapterNumber;
            type Output = ChapterRaw;
            type Error = anyhow::Error;
            fn name() -> String {
                "QueryChapterTool".to_string()
            }
            fn call(
                &self,
                args: Self::Args,
            ) -> impl Future<Output = anyhow::Result<Self::Output>> + Send {
                async move {
                    self.chapters
                        .get(&args)
                        .cloned()
                        .ok_or(anyhow::anyhow!("Chapter not found"))
                }
            }
        }
        let chapter1 = ChapterRaw {
            name: "chapter1".to_string(),
            number: ChapterNumber::from_iter(vec![1]),
            parent_names: vec![],
            path: None,
            content: "this is chapter1".to_string(),
            sub_chapters: vec![],
        };
        let tool = QueryChapterTool {
            chapters: BTreeMap::from_iter([(chapter1.number.clone(), chapter1)]),
        };
        let mut tool_manager = ToolManager::new();
        tool_manager.add_tool(tool);
        let tools = tool_manager.get_tools();
        println!("tools: {:#?}", tools);

        let response = tool_manager
            .call(vec![
                ChatCompletionMessageToolCall {
                    id: "1".to_string(),
                    function: async_openai::types::FunctionCall {
                        name: "QueryChapterTool".to_string(),
                        arguments: serde_json::to_string(&ChapterNumber::from_iter(vec![1]))
                            .unwrap(),
                    },
                    r#type: ChatCompletionToolType::Function,
                }
                .into(),
            ])
            .await;
        println!("{:#?}", response);
        let response = tool_manager
            .call(vec![
                ChatCompletionMessageToolCall {
                    id: "1".to_string(),
                    function: async_openai::types::FunctionCall {
                        name: "QueryChapterTool".to_string(),
                        arguments: serde_json::to_string(&ChapterNumber::from_iter(vec![1, 2]))
                            .unwrap(),
                    },
                    r#type: ChatCompletionToolType::Function,
                }
                .into(),
            ])
            .await;
        println!("{:#?}", response);
    }
}
