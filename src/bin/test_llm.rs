use book_server::llm_fn::{AI_MODEL, OPENAI_API_KEY};
use openai::chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole};

#[tokio::main]
async fn main() {
    let credentials = OPENAI_API_KEY.clone();
    println!("AI_MODEL: {}", AI_MODEL.as_str());
    let mut conversation = vec![];
    loop {
        println!("\n[User]:");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        conversation.push(ChatCompletionMessage {
            role: ChatCompletionMessageRole::User,
            content: Some(input),
            ..Default::default()
        });
        let response = ChatCompletion::builder(AI_MODEL.as_str(), conversation.clone())
            .credentials(credentials.clone())
            .create()
            .await
            .unwrap();
        println!(
            "\n[Grok]:\n{}",
            response.choices[0].message.content.as_ref().unwrap()
        );
        conversation.push(response.choices[0].message.clone());
    }
}
