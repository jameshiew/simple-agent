use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::{command, Parser, Subcommand};
use handlebars::Handlebars;
use ollama_rs::Ollama;
use openai_api_rs::v1::api::OpenAIClient;
use reqwest::Url;
use simple_agent::ollama::OllamaChatProvider;
use simple_agent::openrouter::OpenRouterChatProvider;
use simple_agent::providers::{ChatProvider, ChatProviders};
use simple_agent::run::run_agent;
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

    #[cfg(target_family = "unix")]
    {
        let mut sigterm_stream = signal::unix::signal(signal::unix::SignalKind::terminate())?;
        let mut sigint_stream = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
        tokio::select! {
            result = setup(cli) => {
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
    }
    #[cfg(target_family = "windows")]
    {
        let ctrlc_stream = signal::ctrl_c();
        tokio::select! {
            result = setup(cli) => {
                if let Err(e) = result {
                    bail!(e);
                }
            }
            _ = ctrlc_stream => {
                eprintln!("Received Ctrl-C, shutting down...");
            }
        }
    }

    println!("***Agent finished***");
    Ok(())
}

async fn setup(cli: Cli) -> Result<()> {
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
    let task_rendered = template_registry.render_template(&task_template, &task_values)?;

    let chat_provider = match cli.command {
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

    let first_message = chat_provider.render(&task_rendered);
    println!("---");
    println!("## First request");
    println!("{}", &first_message);
    println!();
    run_agent(chat_provider, first_message).await
}
