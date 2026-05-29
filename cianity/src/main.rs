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
        /// The `.ci` or `.ciane` file to check (defaults to workspace discovery).
        #[arg(conflicts_with = "workspace")]
        file: Option<PathBuf>,
        /// Force workspace root to this directory instead of discovering it.
        #[arg(short = 'w', long, conflicts_with = "file")]
        workspace: Option<PathBuf>,
    },
    /// Build a ciane workflow into CI-system YAML.
    Build {
        /// The `.ci` or `.ciane` file to build (defaults to workspace discovery).
        #[arg(conflicts_with = "workspace")]
        file: Option<PathBuf>,
        /// The target CI system.
        #[arg(short, long)]
        target: Target,
        /// Output file path (defaults to `.gitlab-ci.yml` next to the input file).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Force workspace root to this directory instead of discovering it.
        #[arg(short = 'w', long, conflicts_with = "file")]
        workspace: Option<PathBuf>,
    },
    /// Format one or more ciane workflow files.
    Format {
        /// The `.ci` or `.ciane` files to format (defaults to workspace discovery).
        #[arg(conflicts_with = "workspace")]
        files: Vec<PathBuf>,
        /// Only check whether the files are formatted; do not modify them.
        #[arg(long)]
        check: bool,
        /// Force workspace root to this directory instead of discovering it.
        #[arg(short = 'w', long)]
        workspace: Option<PathBuf>,
    },
    /// Start the ciane language server (communicates over stdin/stdout).
    Lsp {
        /// Use stdin/stdout transport (passed by some LSP clients; always the case).
        #[arg(long)]
        stdio: bool,
    },
}

/// Target CI system for the `build` command.
#[derive(Clone, Copy, ValueEnum)]
pub enum Target {
    /// GitLab CI
    Gitlab,
    /// GitHub Actions
    Github,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Check { file, workspace } => {
            commands::check(file.as_deref(), workspace.as_deref())
        }
        Command::Build {
            file,
            target,
            output,
            workspace,
        } => commands::build(
            file.as_deref(),
            target,
            output.as_deref(),
            workspace.as_deref(),
        ),
        Command::Format {
            files,
            check,
            workspace,
        } => commands::format(&files, check, workspace.as_deref()),
        Command::Lsp { .. } => {
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
