//! The command surface and the dispatch (docs/implementation/08 §CLI contract).
//!
//! Every command from docs/08 is registered here. Commands whose owning
//! milestones have not landed print the empty result envelope and exit 0;
//! `config` is real. The CLI edge in the binary entry point maps errors to
//! exit codes and JSON error envelopes on stderr.

use std::path::PathBuf;

use clap::Subcommand;
use clap::builder::Styles;
use clap::builder::styling;
use devdocs_model::config;
use devdocs_model::{Config, Error, Overrides, empty_envelope};
use serde::Serialize;

/// One-screen help template colors (no color when stdout is not a TTY;
/// clap handles the TTY detection).
fn help_styles() -> Styles {
    styling::Styles::styled()
        .header(styling::AnsiColor::Yellow.on_default())
        .literal(styling::AnsiColor::Green.on_default())
        .placeholder(styling::AnsiColor::Cyan.on_default())
}

/// The `devdocs` command-line interface.
#[derive(Debug, clap::Parser)]
#[command(
    name = "devdocs",
    version,
    about = "Local documentation index for coding agents",
    disable_help_subcommand = true,
    subcommand_required = true,
    arg_required_else_help = true,
    styles = help_styles()
)]
pub struct Cli {
    /// Machine output on stdout; logs stay on stderr.
    #[arg(long, global = true)]
    pub json: bool,

    /// Override the index database path.
    #[arg(long, value_name = "PATH", global = true)]
    pub db: Option<PathBuf>,

    /// Use this config file instead of the project `devdocs.toml`.
    #[arg(long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,

    /// Cap the output budget in tokens.
    #[arg(long, value_name = "N", global = true)]
    pub max_tokens: Option<u64>,

    /// Cap the fetch timeout in milliseconds.
    #[arg(long, value_name = "N", global = true)]
    pub timeout_ms: Option<u64>,

    /// Trace logs on stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Silence diagnostics on stderr.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Every devdocs command (docs/implementation/08 §CLI contract table).
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Ingest a source through the ladder.
    Add {
        /// The source to ingest, for example a GitHub URL.
        source: String,
        /// The version to pin.
        #[arg(long)]
        version: Option<String>,
        /// The source kind override.
        #[arg(long)]
        kind: Option<String>,
    },
    /// Refresh stale approved sources.
    Sync {
        /// Restrict the refresh to one library.
        library: Option<String>,
    },
    /// List indexed libraries and versions (the discovery surface).
    List,
    /// Hybrid search over the local index.
    Search {
        /// The query text.
        query: String,
        /// Restrict to one library.
        #[arg(long)]
        library: Option<String>,
        /// Restrict to one version.
        #[arg(long)]
        version: Option<String>,
        /// Cap the result count.
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Exact symbol lookup.
    Symbol {
        /// The library@version to search.
        target: String,
        /// The symbol name.
        symbol: String,
    },
    /// Read one section by ref.
    Open {
        /// The document ref, for example `docs://next.js/v16.2.9/proxy#rewrite`.
        doc_ref: String,
    },
    /// Re-derive and compare the content hash of a ref.
    Verify {
        /// The document ref to verify.
        doc_ref: String,
    },
    /// List available versions for a library.
    Versions {
        /// The library name.
        library: String,
    },
    /// License-registry operations.
    Source {
        /// The source operation.
        #[command(subcommand)]
        op: SourceOp,
    },
    /// Write the SKILL.md companion file.
    Skill {
        /// The skill operation.
        #[command(subcommand)]
        op: SkillOp,
    },
    /// Start the MCP server on stdio.
    Mcp {
        /// The transport, only `stdio` today.
        #[arg(long, default_value = "stdio")]
        transport: String,
    },
    /// Database operations.
    Db {
        /// The database operation.
        #[command(subcommand)]
        op: DbOp,
    },
    /// Configuration operations.
    Config {
        /// The config operation.
        #[command(subcommand)]
        op: ConfigOp,
    },
    /// Run a golden-set suite.
    Eval {
        /// The eval operation.
        #[command(subcommand)]
        op: EvalOp,
    },
}

/// `devdocs source <op>`.
#[derive(Debug, Subcommand)]
pub enum SourceOp {
    /// Approve a source in the license registry.
    Approve {
        /// The registry entry name.
        name: String,
    },
    /// Deny a source in the license registry.
    Deny {
        /// The registry entry name.
        name: String,
    },
    /// List registry entries.
    List,
}

/// `devdocs skill <op>`.
#[derive(Debug, Subcommand)]
pub enum SkillOp {
    /// Write the SKILL.md companion file.
    Install {
        /// Where to write the file.
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
    },
}

/// `devdocs db <op>`.
#[derive(Debug, Subcommand)]
pub enum DbOp {
    /// Run pending schema migrations.
    Migrate,
    /// Reclaim space in the database file.
    Vacuum,
    /// Print database statistics.
    Stats,
    /// Quarantine operations.
    Quarantine {
        /// The quarantine operation.
        #[command(subcommand)]
        op: QuarantineOp,
    },
}

/// `devdocs db quarantine <op>`.
#[derive(Debug, Subcommand)]
pub enum QuarantineOp {
    /// List quarantined chunks.
    List,
}

/// `devdocs config <op>`.
#[derive(Debug, Subcommand)]
pub enum ConfigOp {
    /// Write the commented user config file.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Print the merged effective configuration as JSON.
    Show,
    /// Validate a config file and report the failing key path.
    Validate {
        /// Validate this file instead of the resolved layers.
        path: Option<PathBuf>,
    },
    /// Print the resolved config paths as JSON.
    Path,
}

/// `devdocs eval <op>`.
#[derive(Debug, Subcommand)]
pub enum EvalOp {
    /// Run one golden-set suite.
    Run {
        /// The suite name.
        suite: String,
    },
}

/// Dispatch the parsed command.
///
/// # Errors
/// Returns the [`Error`] for the CLI edge to map to an exit code.
pub fn run(cli: &Cli) -> Result<(), Error> {
    match &cli.command {
        Command::Config { op } => run_config(cli, op),
        Command::Add { .. }
        | Command::Sync { .. }
        | Command::List
        | Command::Search { .. }
        | Command::Symbol { .. }
        | Command::Open { .. }
        | Command::Verify { .. }
        | Command::Versions { .. }
        | Command::Source { .. }
        | Command::Skill { .. }
        | Command::Mcp { .. }
        | Command::Db { .. }
        | Command::Eval { .. } => print_placeholder(),
    }
}

/// Print the empty result envelope: the deliberate M1 placeholder that the
/// owning milestones replace (docs/implementation/13 §M1).
fn print_placeholder() -> Result<(), Error> {
    print_line(&empty_envelope())
}

/// Print one JSON value as a single line on stdout.
fn print_line<T: Serialize>(value: &T) -> Result<(), Error> {
    let line = serde_json::to_string(value).map_err(|e| Error::Upstream {
        code: devdocs_model::UpstreamCode::UpstreamError,
        detail: format!("cannot serialize the output: {e}"),
    })?;
    println!("{line}");
    Ok(())
}

/// Build the flag layer from the parsed global flags.
fn overrides(cli: &Cli) -> Overrides {
    Overrides {
        config_path: cli.config.clone(),
        db_path: cli.db.clone(),
        max_tokens: cli.max_tokens,
        timeout_ms: cli.timeout_ms,
    }
}

/// Run the `config` subcommands.
fn run_config(cli: &Cli, op: &ConfigOp) -> Result<(), Error> {
    match op {
        ConfigOp::Show => {
            let conf = Config::load(&overrides(cli))?;
            print_line(&conf)
        }
        ConfigOp::Path => {
            let paths = Config::paths(&overrides(cli))?;
            print_line(&paths)
        }
        ConfigOp::Init { force } => config_init(&config::user_config_path()?, *force),
        ConfigOp::Validate { path } => config_validate(path.as_deref()),
    }
}

/// Write the commented user config file; refuse to overwrite without force.
fn config_init(path: &std::path::Path, force: bool) -> Result<(), Error> {
    if path.exists() && !force {
        return Err(Error::Usage(format!(
            "user config already exists at {}; pass --force to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::ConfigInvalid {
            key_path: path.display().to_string(),
            reason: e.to_string(),
        })?;
    }
    std::fs::write(path, user_config_template()).map_err(|e| Error::ConfigInvalid {
        key_path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    println!(
        "{}",
        serde_json::json!({"wrote": path.display().to_string()})
    );
    Ok(())
}

/// Validate one file, or the resolved layer chain when no path is given.
fn config_validate(path: Option<&std::path::Path>) -> Result<(), Error> {
    if let Some(path) = path {
        Config::validate_file(path)?;
        println!(
            "{}",
            serde_json::json!({"valid": true, "path": path.display().to_string()})
        );
    } else {
        // No path given: the merged load validates every layer in turn.
        Config::load(&Overrides::default())?;
        println!("{}", serde_json::json!({"valid": true}));
    }
    Ok(())
}

/// The commented template `config init` writes.
fn user_config_template() -> String {
    String::from(concat!(
        "# devdocs user configuration.\n",
        "# Precedence: flags > DEVDOCS_* environment > project devdocs.toml > this file > defaults.\n",
        "\n",
        "[fetch]\n",
        "concurrency = 4\n",
        "timeout_ms = 15000\n",
        "max_bytes = 20971520\n",
        "max_redirects = 3\n",
        "user_agent = \"devdocs/0.1 (+https://github.com/OWNER/devdocs-mcp)\"\n",
        "\n",
        "[index]\n",
        "# path = \"~/.local/share/devdocs/docs.db\"\n",
        "embeddings = false\n",
        "\n",
        "[search]\n",
        "max_tokens_default = 2000\n",
        "limit_default = 8\n",
        "rrf_k = 60\n",
        "\n",
        "# [sources.nextjs]\n",
        "# kind = \"llms_txt\"\n",
        "# url = \"https://nextjs.org/docs/llms.txt\"\n",
        "# license = \"MIT\"\n",
        "# status = \"approved_snippet\"\n",
        "# refresh_days = 2\n",
    ))
}
