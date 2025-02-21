use anyhow::{Result, bail};
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};

use crate::providers::ChatProvider;

pub struct OpenRouterChatProvider {
    client: OpenAIClient,
    model: String,
    system_prompt: String,
}

impl OpenRouterChatProvider {
    pub fn new(client: OpenAIClient, model: String, system_prompt: String) -> Self {
        Self {
            client,
            model,
            system_prompt,
        }
    }
}

impl ChatProvider for OpenRouterChatProvider {
    async fn send(&mut self, message: &str) -> Result<String> {
        let req = ChatCompletionRequest::new(
            self.model.clone(),
            vec![chat_completion::ChatCompletionMessage {
                role: chat_completion::MessageRole::user,
                content: chat_completion::Content::Text(self.render(message)),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
        );
        let response = self.client.chat_completion(req).await?;
        let content = match &response.choices[0].message.content {
            Some(content) => content,
            None => bail!("no content in response"),
        };
        Ok(content.clone())
    }

    fn render(&self, message: &str) -> String {
        format!("{}\n{}", self.system_prompt, message)
    }
}
