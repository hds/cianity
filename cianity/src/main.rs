mod commands;
mod lsp;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "cianity", version, about = "Add a bit of sanity to your CI.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Statically validate a ciane workflow file.
    Check {
        /// The `.ci` or `.ciane` file to check.
        file: PathBuf,
    },
    /// Build a ciane workflow into CI-system YAML.
    Build {
        /// The `.ci` or `.ciane` file to build.
        file: PathBuf,
        /// The target CI system.
        #[arg(long)]
        target: Target,
    },
    /// Format a ciane workflow file.
    Format {
        /// The `.ci` or `.ciane` file to format.
        file: PathBuf,
        /// Only check whether the file is formatted; do not modify it.
        #[arg(long)]
        check: bool,
    },
    /// Start the ciane language server (communicates over stdin/stdout).
    Lsp,
}

/// Target CI system for the `build` command.
#[derive(Clone, ValueEnum)]
pub enum Target {
    /// GitLab CI
    Gitlab,
    /// GitHub Actions
    Github,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Check { file } => commands::check(&file),
        Command::Build { file, target } => commands::build(&file, target),
        Command::Format { file, check } => commands::format(&file, check),
        Command::Lsp => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build Tokio runtime")
                .block_on(lsp::start());
            return ExitCode::SUCCESS;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
