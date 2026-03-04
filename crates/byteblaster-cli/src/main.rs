//! ByteBlaster CLI - Command-line interface for ByteBlaster protocol.
//!
//! This application provides commands for:
//! - Inspecting capture files
//! - Streaming events from capture files or live servers
//! - Downloading and assembling files
//! - Running an HTTP server with SSE endpoints

mod cmd;
mod output;
mod product_meta;

use clap::{Parser, Subcommand, ValueEnum};

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
enum FormatArg {
    /// Human-readable text output.
    Text,
    /// Machine-readable JSON output.
    Json,
}

/// Color output policy.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorArg {
    /// Enable colors when stdout is a terminal.
    Auto,
    /// Always enable colors.
    Always,
    /// Never enable colors.
    Never,
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
}

/// CLI argument parser for byteblaster.
#[derive(Debug, Parser)]
#[command(name = "byteblaster")]
#[command(about = "ByteBlaster console client")]
struct Cli {
    /// Output format for command results.
    #[arg(long, value_enum, default_value_t = FormatArg::Text)]
    format: FormatArg,
    /// Color output policy.
    #[arg(long, value_enum, default_value_t = ColorArg::Auto)]
    color: ColorArg,
    /// Disable colored output.
    #[arg(long, default_value_t = false)]
    no_color: bool,
    /// Maximum characters for text preview.
    #[arg(long, default_value_t = 80)]
    text_preview_chars: usize,
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let format = match cli.format {
        FormatArg::Text => output::OutputFormat::Text,
        FormatArg::Json => output::OutputFormat::Json,
    };
    let color = if cli.no_color {
        output::ColorPolicy::Never
    } else {
        match cli.color {
            ColorArg::Auto => output::ColorPolicy::Auto,
            ColorArg::Always => output::ColorPolicy::Always,
            ColorArg::Never => output::ColorPolicy::Never,
        }
    };
    output::configure_color(color);
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
            cmd::stream::run(format, input, output_dir, live, text_preview_chars).await
        }
        Commands::Download {
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
        Commands::Inspect { input } => cmd::inspect::run(format, input, text_preview_chars).await,
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
    }
}
