use async_openai::types::{ChatCompletionRequestMessage, CreateChatCompletionRequestArgs};
use book_server::{
    ai_utils::{AI_CLIENT, AI_MODEL},
    utils::init_log,
};
use futures::StreamExt;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc::{self, Sender},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = init_log(None);
    let mut manager = ChatManager::default();
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

#[derive(Debug, Default)]
struct ChatManager {
    conversation: Vec<ChatCompletionRequestMessage>,
}

impl ChatManager {
    async fn chat(&mut self, text: String, tx: Sender<String>) -> anyhow::Result<()> {
        let user_message = ChatCompletionRequestMessage::User(text.into());
        self.conversation.push(user_message);
        let request = CreateChatCompletionRequestArgs::default()
            .model(AI_MODEL.as_str())
            .messages(self.conversation.clone())
            .build()
            .unwrap();
        let mut stream = AI_CLIENT.chat().create_stream(request).await?;
        let mut response_content = String::new();
        while let Some(result) = stream.next().await {
            let response = result?;
            let choice = match response.choices.first() {
                Some(choice) => choice,
                None => continue,
            };
            if let Some(content) = choice.delta.content.as_ref() {
                response_content.push_str(content);
                tx.send(content.clone()).await?;
            }
        }

        self.conversation
            .push(ChatCompletionRequestMessage::Assistant(
                response_content.into(),
            ));

        Ok(())
    }
}
