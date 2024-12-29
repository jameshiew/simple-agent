use anyhow::Result;

use crate::ollama::OllamaChatProvider;
use crate::openrouter::OpenRouterChatProvider;

#[allow(clippy::large_enum_variant)]
pub enum ChatProviders {
    Ollama(OllamaChatProvider),
    OpenRouter(OpenRouterChatProvider),
}

impl ChatProvider for ChatProviders {
    async fn send(&mut self, message: &str) -> Result<String> {
        match self {
            ChatProviders::Ollama(ollama_chat_provider) => ollama_chat_provider.send(message).await,
            ChatProviders::OpenRouter(open_router_chat_provider) => {
                open_router_chat_provider.send(message).await
            }
        }
    }

    fn render(&self, message: &str) -> String {
        match self {
            ChatProviders::Ollama(ollama_chat_provider) => ollama_chat_provider.render(message),
            ChatProviders::OpenRouter(open_router_chat_provider) => {
                open_router_chat_provider.render(message)
            }
        }
    }
}

pub trait ChatProvider {
    #[allow(async_fn_in_trait)]
    async fn send(&mut self, message: &str) -> Result<String>;
    fn render(&self, message: &str) -> String;
}
