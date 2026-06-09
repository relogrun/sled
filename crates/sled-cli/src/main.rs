use anyhow::Result;
use clap::{Parser, Subcommand};
use sled_ai::{Provider, create_model};
use sled_core::{
    DEFAULT_SYSTEM_PROMPT, StepOutcome, WriteOptions, preview_model_input,
    run_until_stop_with_options, say_with_options, status_report, write_default_system_config,
};
use sled_tools::ToolRegistry;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(name = "sled")]
#[command(about = "File-backed AI dialog runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        dir: PathBuf,
    },
    Say {
        dir: PathBuf,
        text: String,
        #[arg(long, env = "SLED_BODY_MIRROR", default_value_t = false)]
        body_mirror: bool,
    },
    Run {
        dir: PathBuf,
        #[arg(long, env = "SLED_PROVIDER", default_value = "operator")]
        provider: Provider,
        #[arg(long, env = "SLED_MODEL")]
        model: Option<String>,
        #[arg(long, env = "SLED_RECENT_K")]
        k: Option<usize>,
        #[arg(long, env = "SLED_BODY_MIRROR", default_value_t = false)]
        body_mirror: bool,
    },
    Status {
        dir: PathBuf,
    },
    Context {
        dir: PathBuf,
        #[arg(long, env = "SLED_RECENT_K")]
        k: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Command::Init { dir } => {
            std::fs::create_dir_all(&dir)?;
            write_default_system_config(&dir)?;
            println!("initialized {}", dir.display());
        }
        Command::Say {
            dir,
            text,
            body_mirror,
        } => {
            let path = say_with_options(&dir, &text, WriteOptions { body_mirror })?;
            println!("wrote {}", path.display());
        }
        Command::Run {
            dir,
            provider,
            model,
            k,
            body_mirror,
        } => {
            let model = create_model(provider, model)?;
            let tools = ToolRegistry::with_defaults();
            match run_until_stop_with_options(
                &dir,
                model.as_ref(),
                &tools,
                DEFAULT_SYSTEM_PROMPT,
                k,
                WriteOptions { body_mirror },
            )
            .await?
            {
                StepOutcome::Waiting(path) => println!("waiting for user: {}", path.display()),
                StepOutcome::Finished(Some(num)) => println!("finished at {num:04}"),
                StepOutcome::Finished(None) => println!("finished"),
                StepOutcome::Continue => unreachable!(),
            }
        }
        Command::Status { dir } => {
            print!("{}", status_report(&dir)?);
        }
        Command::Context { dir, k } => {
            let (system, context) = preview_model_input(&dir, DEFAULT_SYSTEM_PROMPT, k)?;
            println!("=== system ===\n{}\n", system);
            println!("=== index ===\n{}", context.index);
            println!("=== bodies ===\n{}", context.bodies);
        }
    }

    Ok(())
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
}
