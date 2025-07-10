use async_openai::{
    tools::{Tool, ToolCallStreamManager, ToolManager},
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        CreateChatCompletionRequestArgs,
    },
};
use ai_reader::{
    ai_utils::{AI_CLIENT, AI_MODEL},
    utils::init_log,
};
use futures::StreamExt;
use rand::{Rng, rng, seq::IndexedRandom};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc::{self, Sender},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = init_log(None);
    let mut manager = ChatManager::default();
    manager.tools.add_tool(WeatherTool);
    println!("AI_MODEL: {}", AI_MODEL.as_str());
    loop {
        println!("\n[User]:");
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut input = String::new();
        reader.read_line(&mut input).await?;
        let (tx, mut rx) = mpsc::channel(100);
        let (_, results) = unsafe {
            async_scoped::TokioScope::scope_and_collect(|s| {
                s.spawn(async { manager.chat(input.clone(), tx).await });
                s.spawn(async {
                    let mut stdout = tokio::io::stdout();
                    stdout.write_all(b"[Grok]:\n").await?;
                    stdout.flush().await?;
                    while let Some(content) = rx.recv().await {
                        stdout.write_all(content.as_bytes()).await?;
                        stdout.flush().await?;
                    }
                    Ok(())
                });
            })
            .await
        };
        for result in results {
            result??;
        }
    }
}

#[derive(Default)]
struct ChatManager {
    conversation: Vec<ChatCompletionRequestMessage>,
    tools: ToolManager,
}

static MAX_TOOL_CALLS: usize = 10;

impl ChatManager {
    async fn chat(&mut self, text: String, tx: Sender<String>) -> anyhow::Result<()> {
        let user_message = ChatCompletionRequestMessage::User(text.into());
        self.conversation.push(user_message);
        let mut tool_call_count = 0;
        loop {
            let request = CreateChatCompletionRequestArgs::default()
                .model(AI_MODEL.as_str())
                .messages(self.conversation.clone())
                .tools(self.tools.get_tools())
                .build()
                .unwrap();
            let mut stream = AI_CLIENT.chat().create_stream(request).await?;
            let mut response_content = String::new();
            let mut tool_call_stream = ToolCallStreamManager::new();
            while let Some(result) = stream.next().await {
                let response = result?;
                let choice = match response.choices.first() {
                    Some(choice) => choice,
                    None => continue,
                };
                // println!("choice: {:?}", choice);
                if let Some(content) = choice.delta.content.as_ref() {
                    response_content.push_str(content);
                    tx.send(content.clone()).await?;
                }
                if let Some(tool_call_chunks) = &choice.delta.tool_calls {
                    tool_call_stream.process_chunks(tool_call_chunks.clone());
                }
            }
            let mut message_builder = ChatCompletionRequestAssistantMessageArgs::default();
            if !response_content.is_empty() {
                message_builder.content(response_content);
            }
            let tool_calls = tool_call_stream.finish_stream();
            if !tool_calls.is_empty() {
                message_builder.tool_calls(tool_calls.clone());
            }
            self.conversation.push(message_builder.build()?.into());
            if tool_calls.is_empty() || tool_call_count >= MAX_TOOL_CALLS {
                break;
            }
            tool_call_count += 1;
            println!("tool_calls: {:?}", tool_calls);
            let tool_results = self.tools.call(tool_calls).await;
            println!("tool_results: {:?}", tool_results);
            self.conversation
                .extend(tool_results.into_iter().map(|t| t.into()));
        }
        Ok(())
    }
}

#[derive(Debug, JsonSchema, Deserialize, Serialize)]
enum Unit {
    Fahrenheit,
    Celsius,
}

#[derive(Debug, JsonSchema, Deserialize)]
struct WeatherRequest {
    /// The city and state, e.g. San Francisco, CA
    location: String,
    unit: Unit,
}

#[derive(Debug, Serialize)]
struct WeatherResponse {
    location: String,
    temperature: String,
    unit: Unit,
    forecast: String,
}

struct WeatherTool;

impl Tool for WeatherTool {
    type Args = WeatherRequest;
    type Output = WeatherResponse;
    type Error = anyhow::Error;

    fn name() -> String {
        "get_current_weather".to_string()
    }

    fn description() -> Option<String> {
        Some("Get the current weather in a given location".to_string())
    }

    async fn call(&self, args: Self::Args) -> anyhow::Result<Self::Output> {
        let mut rng = rng();

        let temperature: i32 = rng.random_range(20..=55);

        let forecasts = [
            "sunny", "cloudy", "overcast", "rainy", "windy", "foggy", "snowy",
        ];

        let forecast = forecasts.choose(&mut rng).unwrap_or(&"sunny");

        let weather_info = WeatherResponse {
            location: args.location,
            temperature: temperature.to_string(),
            unit: args.unit,
            forecast: forecast.to_string(),
        };

        Ok(weather_info)
    }
}
