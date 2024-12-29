use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::{command, Parser};
use handlebars::Handlebars;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage, MessageRole};
use ollama_rs::Ollama;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[arg(long, help = "File containing the task to execute")]
    task: PathBuf,
    #[arg(long, help = "The model to use")]
    model: String,
    #[arg(long, short)]
    ollama: Url,
    #[arg(long, help = "The path to the system prompt")]
    system: PathBuf,
    #[arg(
        long,
        help = "The path to the Handlebars template that will wrap the task"
    )]
    template: PathBuf,
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

    tokio::select! {
        result = run_agent(cli) => {
            if let Err(e) = result {
                bail!(e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("CTRL-C received, shutting down");
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
    let model = cli.model.clone();
    let ollama = Ollama::from_url(cli.ollama);
    let models = ollama
        .list_local_models()
        .await
        .with_context(|| "couldn't list available models, is Ollama running and reachable?")?;
    if !models.into_iter().any(|m| m.name == model) {
        bail!("Model {} not found", model);
    }

    let task = fs::read_to_string(cli.task)
        .await
        .with_context(|| "Failed to read task")?;
    let system = fs::read_to_string(cli.system)
        .await
        .with_context(|| "Failed to read system")?;
    let task_template = fs::read_to_string(cli.template)
        .await
        .with_context(|| "Failed to read template")?;
    let mut template_registry = Handlebars::new();
    template_registry.register_escape_fn(|s| s.to_string());

    let task_values = HashMap::from([("task", task)]);
    let mut message = template_registry.render_template(&task_template, &task_values)?;
    let mut ollama = OllamaChatProvider::new(ollama, cli.model.clone(), system);

    println!("Model: {}", model);
    println!("Chat ID: {}", ollama.chat_id);

    let first_prompt = ollama.render(&message);
    println!("---");
    println!("## First request");
    println!("{}", &first_prompt);
    println!();
    println!("> Sending first request to Ollama (may take a short while while model is loaded)");
    let mut i = 0;
    loop {
        i += 1;
        let response = ollama.send(&message).await?;
        println!("## Response {}", i);
        println!("{}", response);
        println!();
        let output = match parse(&response) {
            Ok(response) => {
                if response.run.trim_ascii_start().trim_ascii_end().eq("STOP") {
                    None
                } else {
                    let mut cmd = Command::new("bash");
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
