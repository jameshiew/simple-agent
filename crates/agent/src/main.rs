use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::{command, Parser, Subcommand};
use handlebars::Handlebars;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage, MessageRole};
use ollama_rs::Ollama;
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::signal::unix::SignalKind;
use tokio::{fs, signal};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
    #[arg(
        long,
        global = true,
        default_value = "task.md",
        help = "File containing the task to execute"
    )]
    task: PathBuf,
    #[arg(
        long,
        global = true,
        default_value = "system.md",
        help = "The path to the system prompt"
    )]
    system: PathBuf,
    #[arg(
        long,
        global = true,
        default_value = "task_template.hbs",
        help = "The path to the Handlebars template that will wrap the task"
    )]
    task_template: PathBuf,
}

#[derive(Debug, Subcommand, Clone)]
enum Command {
    Ollama {
        #[arg(long, help = "The model to use")]
        model: String,
        #[arg(long, short)]
        url: Url,
    },
    Openrouter {
        #[arg(long, help = "The model to use")]
        model: String,
        #[arg(long, short, default_value = "https://openrouter.ai/api/v1")]
        url: Url,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("***Agent started***");
    let cli = Cli::parse();
    println!("Arguments: {:#?}", &cli);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .expect("should be able to initialize the logger");
    tracing::info!("Starting up");

    let mut sigterm_stream = signal::unix::signal(SignalKind::terminate())?;
    let mut sigint_stream = signal::unix::signal(SignalKind::interrupt())?;
    tokio::select! {
        result = run_agent(cli) => {
            if let Err(e) = result {
                bail!(e);
            }
        }
        _ = sigterm_stream.recv() => {
            eprintln!("Received SIGTERM, shutting down...");
        }
        _ = sigint_stream.recv() => {
            eprintln!("Received SIGINT (Ctrl-C), shutting down...");
        }
    }

    println!("***Agent finished***");
    Ok(())
}

struct OllamaChatProvider {
    client: Ollama,
    model: String,
    system_prompt: String,
    chat_id: String,
}

impl OllamaChatProvider {
    fn new(client: Ollama, model: String, system_prompt: String) -> Self {
        Self {
            client,
            model,
            system_prompt,
            chat_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

struct OpenRouterChatProvider {
    client: OpenAIClient,
    model: String,
    system_prompt: String,
}

impl OpenRouterChatProvider {
    fn new(client: OpenAIClient, model: String, system_prompt: String) -> Self {
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
                content: chat_completion::Content::Text(String::from(self.render(&message))),
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

enum ChatProviders {
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

trait ChatProvider {
    async fn send(&mut self, message: &str) -> Result<String>;
    fn render(&self, message: &str) -> String;
}

impl ChatProvider for OllamaChatProvider {
    async fn send(&mut self, message: &str) -> Result<String> {
        let msg = ChatMessage {
            role: MessageRole::User,
            content: self.render(&message),
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

async fn run_agent(cli: Cli) -> Result<()> {
    let task = fs::read_to_string(cli.task)
        .await
        .with_context(|| "failed to read task")?;
    let system = fs::read_to_string(cli.system)
        .await
        .with_context(|| "failed to read system")?;
    let task_template = fs::read_to_string(cli.task_template)
        .await
        .with_context(|| "failed to read template")?;
    let mut template_registry = Handlebars::new();
    template_registry.register_escape_fn(|s| s.to_string());
    let task_values = HashMap::from([("task", task)]);
    let mut message = template_registry.render_template(&task_template, &task_values)?;

    let mut chat_provider = match cli.command {
        Command::Ollama { model, url } => {
            let model = model.clone();
            let ollama = Ollama::from_url(url);
            let models = ollama.list_local_models().await.with_context(|| {
                "couldn't list available models, is Ollama running and reachable?"
            })?;
            if !models.into_iter().any(|m| m.name == model) {
                bail!("model {} not found", model);
            }
            let ollama = OllamaChatProvider::new(ollama, model.clone(), system);

            println!("Model: {}", model);
            println!("Chat ID: {}", ollama.chat_id);
            ChatProviders::Ollama(ollama)
        }
        Command::Openrouter { model, url } => {
            let api_key = std::env::var("OPENROUTER_API_KEY")
                .with_context(|| "OPENROUTER_API_KEY not found in environment")?;
            let openrouter = OpenAIClient::builder()
                .with_api_key(api_key)
                .with_endpoint(url)
                .build()
                .map_err(|_e| anyhow!("couldn't build OpenRouter client"))?;
            let openrouter = OpenRouterChatProvider::new(openrouter, model, system);
            ChatProviders::OpenRouter(openrouter)
        }
    };

    let first_prompt = chat_provider.render(&message);
    println!("---");
    println!("## First request");
    println!("{}", &first_prompt);
    println!();
    println!("> Sending first request (may take a short while if using Ollama)");
    let mut i = 0;
    loop {
        i += 1;
        let response = chat_provider.send(&message).await?;
        println!("## Response {}", i);
        println!("{}", response);
        println!();
        let output = match parse(&response) {
            Ok(response) => {
                if response.run.trim_ascii_start().trim_ascii_end().eq("STOP") {
                    None
                } else {
                    let mut cmd = tokio::process::Command::new("bash");
                    cmd.arg("-c");
                    cmd.args(vec![response.run]);
                    match cmd.output().await {
                        Ok(output) => {
                            let stdout = String::from_utf8(output.stdout.clone())?;
                            tracing::debug!(stdout, "stdout");
                            let stderr = String::from_utf8(output.stderr.clone())?;
                            tracing::debug!(stderr, "stderr");
                            Some(CommandOutput {
                                stdout,
                                stderr,
                                exit_code: output.status.code(),
                            })
                        }
                        Err(err) => {
                            println!("## Error trying to run command");
                            println!();
                            println!("{}", err);
                            Some(CommandOutput {
                                stdout: "Error trying to run command".to_string(),
                                stderr: err.to_string(),
                                exit_code: None,
                            })
                        }
                    }
                }
            }
            Err(err) => {
                println!("## Error parsing response");
                println!();
                println!("{}", err);
                Some(CommandOutput {
                    stdout: "Error parsing response".to_string(),
                    stderr: err.to_string(),
                    exit_code: None,
                })
            }
        };
        let Some(output) = output else {
            break;
        };

        message = serde_yml::to_string(&output)?;

        println!("---");
        println!("## Request {}", i);
        println!("{}", message);
        println!();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Response {
    #[allow(dead_code)] // only used during deserialization
    thoughts: Vec<String>,
    run: String,
}

#[derive(Debug, Serialize)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

fn parse(content: &str) -> Result<Response> {
    let content = content.trim_ascii_start().trim_ascii_end();
    let content = content.strip_prefix("```yaml").unwrap_or(content);
    let content = content.strip_prefix("```yml").unwrap_or(content);
    let content = content.strip_suffix("```").unwrap_or(content);
    let parsed: Response = serde_yml::from_str(content)?;
    Ok(parsed)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse() -> Result<()> {
        let content = r#"
thoughts:
  - "This is a thought"
  - "This is another thought"
run: "ls -la""#;

        let Response {
            thoughts,
            run: command,
        } = parse(content)?;
        assert_eq!(
            thoughts,
            vec!["This is a thought", "This is another thought"]
        );
        assert_eq!(command, "ls -la");
        Ok(())
    }

    #[test]
    fn test_parse_code_fence() -> Result<()> {
        let content = r#"```yaml
thoughts:
  - "This is a thought"
  - "This is another thought"
run: "ls -la"
```"#;

        let Response {
            thoughts,
            run: command,
        } = parse(content)?;
        assert_eq!(
            thoughts,
            vec!["This is a thought", "This is another thought"]
        );
        assert_eq!(command, "ls -la");
        Ok(())
    }

    #[test]
    fn test_parse_code_multiline() -> Result<()> {
        let content = r#"```yaml
thoughts:
  - "This is a thought"
  - "This is another thought"
run: |
    ls -la
    echo 'hello' > hello.txt
```"#;

        let Response {
            thoughts,
            run: command,
        } = parse(content)?;
        assert_eq!(
            thoughts,
            vec!["This is a thought", "This is another thought"]
        );
        assert_eq!(command, "ls -la\necho 'hello' > hello.txt\n");
        Ok(())
    }
}
