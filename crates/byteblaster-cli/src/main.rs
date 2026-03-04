//! ByteBlaster CLI - Command-line interface for ByteBlaster protocol.
//!
//! This application provides commands for:
//! - Inspecting capture files
//! - Streaming events from capture files or live servers
//! - Downloading and assembling files
//! - Running an HTTP server with SSE endpoints

mod cmd;
mod live;
mod product_meta;
mod relay;

use clap::{Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use tracing_subscriber::EnvFilter;

/// Options for live mode connections.
#[derive(Debug, Clone)]
struct LiveOptions {
    /// User email for authentication.
    email: Option<String>,
    /// Custom server endpoints.
    servers: Vec<String>,
    /// Path to persisted server list.
    server_list_path: Option<String>,
    /// Maximum number of events to process.
    max_events: usize,
    /// Idle timeout before disconnecting (in seconds).
    idle_timeout_secs: u64,
}

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum OutputFormat {
    /// Human-readable text output.
    Text,
    /// Machine-readable JSON output.
    Json,
}

/// Available CLI commands.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Stream events from a capture file or live server.
    Stream {
        /// Path to capture file (omit for live mode).
        input: Option<String>,
        /// Optional directory to write completed files.
        #[arg(long)]
        output_dir: Option<String>,
        /// Email address for live mode authentication.
        #[arg(long)]
        email: Option<String>,
        /// Custom server endpoints (comma-separated or multiple).
        #[arg(long = "server", value_delimiter = ',')]
        servers: Vec<String>,
        /// Path to persisted server list file.
        #[arg(long)]
        server_list_path: Option<String>,
        /// Maximum number of events to process.
        #[arg(long)]
        max_events: Option<usize>,
        /// Idle timeout in seconds.
        #[arg(long, default_value_t = 20)]
        idle_timeout_secs: u64,
    },
    /// Download and assemble files from capture or live server.
    Download {
        /// Output format for command results.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Output directory for downloaded files.
        output_dir: String,
        /// Path to capture file (omit for live mode).
        input: Option<String>,
        /// Email address for live mode authentication.
        #[arg(long)]
        email: Option<String>,
        /// Custom server endpoints (comma-separated or multiple).
        #[arg(long = "server", value_delimiter = ',')]
        servers: Vec<String>,
        /// Path to persisted server list file.
        #[arg(long)]
        server_list_path: Option<String>,
        /// Maximum number of events to process.
        #[arg(long, default_value_t = 200)]
        max_events: usize,
        /// Idle timeout in seconds.
        #[arg(long, default_value_t = 20)]
        idle_timeout_secs: u64,
    },
    /// Inspect and decode a capture file.
    Inspect {
        /// Output format for command results.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Path to capture file (omit to read from stdin).
        input: Option<String>,
    },
    /// Run HTTP server with SSE endpoints.
    Server {
        /// Email address for authentication.
        #[arg(long)]
        email: String,
        /// Custom server endpoints (comma-separated or multiple).
        #[arg(long = "server", value_delimiter = ',')]
        servers: Vec<String>,
        /// Path to persisted server list file.
        #[arg(long)]
        server_list_path: Option<String>,
        /// Bind address for the HTTP server.
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
        /// CORS origin header (use "*" for any).
        #[arg(long)]
        cors_origin: Option<String>,
        /// Maximum concurrent SSE clients.
        #[arg(long, default_value_t = 100)]
        max_clients: usize,
        /// Stats logging interval in seconds (0 to disable).
        #[arg(long, default_value_t = 30)]
        stats_interval_secs: u64,
        /// File retention time in seconds.
        #[arg(long, default_value_t = 300)]
        file_retention_secs: u64,
        /// Maximum number of retained files.
        #[arg(long, default_value_t = 1000)]
        max_retained_files: usize,
        /// Suppress non-error output.
        #[arg(long, default_value_t = false)]
        quiet: bool,
    },
    /// Run low-latency ByteBlaster passthrough relay.
    Relay {
        #[command(flatten)]
        options: relay::RelayOptions,
    },
}

/// CLI argument parser for byteblaster.
#[derive(Debug, Parser)]
#[command(name = "byteblaster")]
#[command(about = "ByteBlaster console client")]
struct Cli {
    /// Maximum characters for text preview.
    #[arg(long, default_value_t = 80)]
    text_preview_chars: usize,
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging();
    let text_preview_chars = cli.text_preview_chars;

    match cli.command {
        Commands::Stream {
            input,
            output_dir,
            email,
            servers,
            server_list_path,
            max_events,
            idle_timeout_secs,
        } => {
            let live = LiveOptions {
                email,
                servers,
                server_list_path,
                max_events: max_events.unwrap_or(usize::MAX),
                idle_timeout_secs,
            };
            cmd::stream::run(input, output_dir, live, text_preview_chars).await
        }
        Commands::Download {
            format,
            output_dir,
            input,
            email,
            servers,
            server_list_path,
            max_events,
            idle_timeout_secs,
        } => {
            let live = LiveOptions {
                email,
                servers,
                server_list_path,
                max_events,
                idle_timeout_secs,
            };
            cmd::download::run(format, output_dir, input, live, text_preview_chars).await
        }
        Commands::Inspect { format, input } => {
            cmd::inspect::run(format, input, text_preview_chars).await
        }
        Commands::Server {
            email,
            servers,
            server_list_path,
            bind,
            cors_origin,
            max_clients,
            stats_interval_secs,
            file_retention_secs,
            max_retained_files,
            quiet,
        } => {
            let options = cmd::server::ServerOptions {
                email,
                raw_servers: servers,
                server_list_path,
                bind,
                cors_origin,
                max_clients,
                stats_interval_secs,
                file_retention_secs,
                max_retained_files,
                quiet,
            };
            cmd::server::run(options).await
        }
        Commands::Relay { options } => relay::run(options).await,
    }
}

fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let ansi = match std::env::var("RUST_LOG_STYLE") {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "always" => true,
            "never" => false,
            _ => std::io::stderr().is_terminal(),
        },
        Err(_) => std::io::stderr().is_terminal(),
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_ansi(ansi)
        .try_init();
}
