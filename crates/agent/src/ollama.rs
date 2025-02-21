use anyhow::{Result, anyhow};
use ollama_rs::Ollama;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage, MessageRole};

use crate::providers::ChatProvider;

pub struct OllamaChatProvider {
    pub client: Ollama,
    pub model: String,
    pub system_prompt: String,
    pub chat_id: String,
}

impl OllamaChatProvider {
    pub fn new(client: Ollama, model: String, system_prompt: String) -> Self {
        Self {
            client,
            model,
            system_prompt,
            chat_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

impl ChatProvider for OllamaChatProvider {
    async fn send(&mut self, message: &str) -> Result<String> {
        let msg = ChatMessage {
            role: MessageRole::User,
            content: self.render(message),
            images: None,
        };
        let request = ChatMessageRequest::new(self.model.clone(), vec![msg]);
        let response = self
            .client
            .send_chat_messages_with_history(request, &self.chat_id)
            .await?;
        response
            .message
            .ok_or_else(|| anyhow!("no message received from Ollama"))
            .map(|m| m.content)
    }

    fn render(&self, message: &str) -> String {
        format!("{}\n{}", self.system_prompt, message)
    }
}
