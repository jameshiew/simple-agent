use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::providers::ChatProvider;

pub async fn run_agent(mut chat_provider: impl ChatProvider, mut message: String) -> Result<()> {
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
