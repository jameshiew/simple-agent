use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
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

async fn run_agent(cli: Cli) -> Result<()> {
    let model = cli.model.clone();
    let mut ollama = Ollama::from_url(cli.ollama);
    let models = ollama
        .list_local_models()
        .await
        .with_context(|| "couldn't list available models, is Ollama running and reachable?")?;
    if !models.into_iter().any(|m| m.name == model) {
        bail!("Model {} not found", model);
    }
    let chat_id = uuid::Uuid::new_v4().to_string();
    println!("Model: {}", model);
    println!("Chat ID: {}", chat_id);

    let task = fs::read_to_string(cli.task)
        .await
        .with_context(|| "Failed to read task")?;
    let system = fs::read_to_string(cli.system)
        .await
        .with_context(|| "Failed to read system")?;
    let template = fs::read_to_string(cli.template)
        .await
        .with_context(|| "Failed to read template")?;

    let mut reg = Handlebars::new();
    reg.register_escape_fn(|s| s.to_string());

    let values = HashMap::from([("task", task)]);
    let rendered = reg.render_template(&template, &values)?;

    let formatted = format!("{}\n{}", system, rendered);
    println!("## First request");
    println!("{}", &formatted);
    println!();
    let initial_message = ChatMessage {
        role: MessageRole::User,
        content: formatted,
        images: None,
    };
    let mut request = ChatMessageRequest::new(model.clone(), vec![initial_message]);
    println!("Sending first request to Ollama (may take a short while while model is loaded)");
    loop {
        let response = ollama
            .send_chat_messages_with_history(request, &chat_id)
            .await?;
        let Some(message) = response.message else {
            bail!("No message received from Ollama");
        };
        println!("## Response {}", response.created_at);
        println!("{}", message.content);
        println!();
        let output = match parse(&message.content) {
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

        let formatted = serde_yml::to_string(&output)?;

        println!("---");
        println!("## Request {}", response.created_at);
        println!("{}", formatted);
        println!();
        tracing::debug!(%formatted, "formatted");
        let formatted = format!("{}\n{}", system, formatted);
        request = ChatMessageRequest::new(
            model.clone(),
            vec![ChatMessage::new(MessageRole::User, formatted)],
        );
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
