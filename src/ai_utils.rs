use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
    pin::Pin,
    sync::{Arc, LazyLock},
};

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
        ChatCompletionNamedToolChoice, ChatCompletionRequestMessage,
        ChatCompletionRequestToolMessage, ChatCompletionTool, ChatCompletionToolChoiceOption,
        ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall, FunctionCallStream,
        FunctionName, FunctionObject,
    },
};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::error;

pub static AI_MODEL: LazyLock<String> = LazyLock::new(|| {
    let _ = dotenvy::dotenv();
    dotenvy::var("AI_MODEL").unwrap()
});

pub static AI_CLIENT: LazyLock<Client<OpenAIConfig>> = LazyLock::new(|| {
    let _ = dotenvy::dotenv();
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

pub trait Tool: Send + Sync {
    type Args: JsonSchema + for<'a> Deserialize<'a> + Send + Sync;
    type Output: Serialize + Send + Sync;
    fn name(&self) -> String {
        Self::Args::schema_name()
    }
    fn description(&self) -> Option<String> {
        None
    }
    fn parameters(&self) -> serde_json::Value {
        json!(schema_for!(Self::Args))
    }
    fn definition(&self) -> ChatCompletionTool {
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: self.name(),
                description: self.description(),
                parameters: Some(self.parameters()),
                strict: None,
            },
        }
    }
    fn call(&self, args: Self::Args) -> impl Future<Output = anyhow::Result<Self::Output>> + Send;
}

pub trait ToolDyn: Send + Sync {
    fn name(&self) -> String
    where
        Self: Sized;
    fn definition(&self) -> ChatCompletionTool;
    fn call(
        &self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>>;
}
impl<T: Tool> ToolDyn for T {
    fn name(&self) -> String {
        T::name(self)
    }
    fn definition(&self) -> ChatCompletionTool {
        T::definition(self)
    }
    fn call(
        &self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>> {
        let future = async move {
            match serde_json::from_str::<T::Args>(&args) {
                Ok(args) => T::call(self, args).await.and_then(|output| {
                    serde_json::to_string(&output)
                        .map_err(|e| anyhow::anyhow!("Failed to serialize output: {}", e))
                }),
                Err(e) => Err(anyhow::anyhow!("Failed to parse arguments: {}", e)),
            }
        };
        Box::pin(future)
    }
}
#[derive(Default)]
pub struct ToolManager {
    tools: BTreeMap<String, Arc<dyn ToolDyn>>,
}

impl ToolManager {
    pub fn add_tool(&mut self, tool: impl Tool + 'static) {
        self.tools.insert(tool.name(), Arc::new(tool));
    }
    pub fn add_tools(&mut self, tools: impl IntoIterator<Item = impl Tool + 'static>) {
        for tool in tools {
            self.add_tool(tool);
        }
    }
    pub fn get_tools(&self) -> Vec<ChatCompletionTool> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }
    pub async fn call(
        &self,
        calls: impl IntoIterator<Item = ChatCompletionMessageToolCall>,
    ) -> Vec<ChatCompletionRequestToolMessage> {
        let mut handles = Vec::new();

        let mut outputs = Vec::new();
        // Spawn a task for each tool call
        for call in calls {
            if let Some(tool) = self.tools.get(&call.function.name).cloned() {
                let handle = tokio::spawn(async move { tool.call(call.function.arguments).await });
                handles.push((call.id, handle));
            } else {
                outputs.push(ChatCompletionRequestToolMessage {
                    content: "Tool not found".into(),
                    tool_call_id: call.id,
                });
            }
        }
        // Collect results from all spawned tasks
        for (id, handle) in handles {
            let output = match handle.await {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => {
                    error!("Tool call {} failed: {}", id, e);
                    e.to_string()
                }
                Err(e) => {
                    error!("Tool call {} Join error: {}", id, e);
                    continue;
                }
            };
            outputs.push(ChatCompletionRequestToolMessage {
                content: output.into(),
                tool_call_id: id,
            });
        }
        outputs
    }
}

pub async fn summarize(content: &str, limit: usize) -> anyhow::Result<String> {
    let prompt = format!(
        "Provide a concise summary of the following text in {} words or less. Return only the summary without any additional text or explanation:\n{}",
        limit, content
    );
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
        .get(0)
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

#[derive(Default, Clone, Debug)]
pub struct ToolCallStreamManager(HashMap<u32, ChatCompletionMessageToolCall>);

impl ToolCallStreamManager {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
    pub fn merge_chunk(&mut self, chunk: ChatCompletionMessageToolCallChunk) {
        match self.0.entry(chunk.index) {
            Entry::Occupied(mut o) => {
                if let Some(FunctionCallStream {
                    name: _,
                    arguments: Some(arguments),
                }) = chunk.function
                {
                    o.get_mut().function.arguments.push_str(&arguments);
                }
            }
            Entry::Vacant(o) => {
                let ChatCompletionMessageToolCallChunk {
                    index: _,
                    id: Some(id),
                    r#type: _,
                    function:
                        Some(FunctionCallStream {
                            name: Some(name),
                            arguments: Some(arguments),
                        }),
                } = chunk
                else {
                    tracing::error!("Tool call chunk is not complete: {:?}", chunk);
                    return;
                };
                let tool_call = ChatCompletionMessageToolCall {
                    id,
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionCall { name, arguments },
                };
                o.insert(tool_call);
            }
        }
    }
    pub fn merge_chunks(
        &mut self,
        chunks: impl IntoIterator<Item = ChatCompletionMessageToolCallChunk>,
    ) {
        for chunk in chunks {
            self.merge_chunk(chunk);
        }
    }
    pub fn get_tool_calls(self) -> Vec<ChatCompletionMessageToolCall> {
        self.0.into_values().collect()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        book::chapter::{Chapter, ChapterNumber},
        utils::init_log,
    };

    #[tokio::test]
    async fn test_tool_manager() {
        let _guard = init_log(None);
        struct QueryChapterTool {
            chapters: BTreeMap<ChapterNumber, Chapter>,
        }
        impl Tool for QueryChapterTool {
            type Args = ChapterNumber;
            type Output = Chapter;
            fn name(&self) -> String {
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
        let chapter1 = Chapter {
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
        let mut tool_manager = ToolManager::default();
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
