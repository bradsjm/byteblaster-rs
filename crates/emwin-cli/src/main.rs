//! EMWIN CLI - Command-line interface for EMWIN protocol.
//!
//! This application provides commands for:
//! - Streaming events from live servers
//! - Running an HTTP server with SSE endpoints

mod cmd;
mod default_servers;
mod error;
mod live;
mod relay;

use clap::{Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use tracing_subscriber::EnvFilter;

/// Options for live mode connections.
#[derive(Debug, Clone)]
struct LiveOptions {
    /// Selected upstream receiver runtime.
    receiver: ReceiverKind,
    /// Account username for authentication.
    username: Option<String>,
    /// Receiver password when required by backend.
    password: Option<String>,
    /// Custom server endpoints.
    servers: Vec<String>,
    /// Path to persisted server list.
    server_list_path: Option<String>,
    /// Product/file metadata filter compiled from --filter flags.
    file_filter: Option<crate::live::filter::FileEventFilter>,
    /// Maximum number of events to process.
    max_events: usize,
    /// Idle timeout before disconnecting (in seconds).
    idle_timeout_secs: u64,
}

/// Supported upstream receiver backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReceiverKind {
    /// QBT/EMWIN TCP receiver.
    Qbt,
    /// Weather Wire XMPP receiver.
    Wxwire,
}

/// Available CLI commands.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Stream events from a live server.
    Stream {
        /// Optional directory to write completed files.
        #[arg(long)]
        output_dir: Option<String>,
        /// Account username for live mode authentication.
        #[arg(long, env = "EMWIN_USERNAME")]
        username: Option<String>,
        /// Password for receivers that require one (for example wxwire).
        #[arg(long, env = "EMWIN_PASSWORD")]
        password: Option<String>,
        /// Receiver backend to use in live mode.
        #[arg(long, value_enum, env = "EMWIN_RECEIVER", default_value_t = ReceiverKind::Qbt)]
        receiver: ReceiverKind,
        /// Custom server endpoints (comma-separated or multiple).
        #[arg(long = "server", env = "EMWIN_SERVER", value_delimiter = ',')]
        servers: Vec<String>,
        /// Path to persisted server list file.
        #[arg(long, env = "EMWIN_SERVER_LIST_PATH")]
        server_list_path: Option<String>,
        /// Repeatable file metadata filters using server /events field names.
        #[arg(long = "filter")]
        filters: Vec<String>,
        /// Maximum number of events to process.
        #[arg(long, env = "EMWIN_MAX_EVENTS")]
        max_events: Option<usize>,
        /// Idle timeout in seconds.
        #[arg(long, env = "EMWIN_IDLE_TIMEOUT_SECS", default_value_t = 90)]
        idle_timeout_secs: u64,
    },
    /// Run HTTP server with SSE endpoints.
    Server {
        /// Account username for authentication.
        #[arg(long, env = "EMWIN_USERNAME")]
        username: String,
        /// Password for receivers that require one (for example wxwire).
        #[arg(long, env = "EMWIN_PASSWORD")]
        password: Option<String>,
        /// Receiver backend to use.
        #[arg(long, value_enum, env = "EMWIN_RECEIVER", default_value_t = ReceiverKind::Qbt)]
        receiver: ReceiverKind,
        /// Custom server endpoints (comma-separated or multiple).
        #[arg(long = "server", env = "EMWIN_SERVER", value_delimiter = ',')]
        servers: Vec<String>,
        /// Path to persisted server list file.
        #[arg(long, env = "EMWIN_SERVER_LIST_PATH")]
        server_list_path: Option<String>,
        /// Bind address for the HTTP server.
        #[arg(long, env = "EMWIN_BIND", default_value = "127.0.0.1:8080")]
        bind: String,
        /// CORS origin header (use "*" for any).
        #[arg(long, env = "EMWIN_CORS_ORIGIN")]
        cors_origin: Option<String>,
        /// Maximum concurrent SSE clients.
        #[arg(long, env = "EMWIN_MAX_CLIENTS", default_value_t = 100)]
        max_clients: usize,
        /// Stats logging interval in seconds (0 to disable).
        #[arg(long, env = "EMWIN_STATS_INTERVAL_SECS", default_value_t = 30)]
        stats_interval_secs: u64,
        /// File retention time in seconds.
        #[arg(long, env = "EMWIN_FILE_RETENTION_SECS", default_value_t = 300)]
        file_retention_secs: u64,
        /// Maximum number of retained files.
        #[arg(long, env = "EMWIN_MAX_RETAINED_FILES", default_value_t = 1000)]
        max_retained_files: usize,
        /// Suppress non-error output.
        #[arg(long, env = "EMWIN_QUIET", default_value_t = false)]
        quiet: bool,
    },
    /// Run low-latency EMWIN passthrough relay.
    Relay {
        #[command(flatten)]
        options: relay::RelayOptions,
    },
}

/// CLI argument parser for emwin.
#[derive(Debug, Parser)]
#[command(name = "emwin")]
#[command(about = "EMWIN console client")]
struct Cli {
    /// Maximum characters for text preview.
    #[arg(long, env = "EMWIN_TEXT_PREVIEW_CHARS", default_value_t = 80)]
    text_preview_chars: usize,
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> crate::error::CliResult<()> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    init_logging();
    let text_preview_chars = cli.text_preview_chars;

    match cli.command {
        Commands::Stream {
            output_dir,
            username,
            password,
            receiver,
            servers,
            server_list_path,
            filters,
            max_events,
            idle_timeout_secs,
        } => {
            let live = LiveOptions {
                receiver,
                username,
                password,
                servers,
                server_list_path,
                file_filter: crate::live::filter::FileEventFilter::from_cli_filters(&filters)?,
                max_events: max_events.unwrap_or(usize::MAX),
                idle_timeout_secs,
            };
            live::stream::run(output_dir, live, text_preview_chars).await
        }
        Commands::Server {
            username,
            password,
            receiver,
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
            let options = live::server::ServerOptions {
                username,
                password,
                receiver,
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
            live::server::run(options).await
        }
        Commands::Relay { options } => relay::runtime::run(options).await,
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

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::{CommandFactory, Parser};

    #[test]
    fn root_help_does_not_list_download() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("stream"));
        assert!(!help.contains("download"));
    }

    #[test]
    fn stream_help_mentions_output_dir_but_not_download() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("stream")
            .expect("stream subcommand should exist")
            .render_long_help()
            .to_string();

        assert!(help.contains("--output-dir"));
        assert!(!help.contains("download"));
    }

    #[test]
    fn download_subcommand_is_rejected() {
        let error = Cli::try_parse_from(["emwin", "download", "./out"])
            .expect_err("download subcommand should be rejected");

        assert!(
            error
                .to_string()
                .contains("unrecognized subcommand 'download'")
        );
    }

    #[test]
    fn stream_default_max_events_is_unbounded() {
        let cli = Cli::try_parse_from(["emwin", "stream"]).expect("stream args should parse");

        let Commands::Stream { max_events, .. } = cli.command else {
            panic!("expected stream command");
        };

        assert_eq!(max_events, None);
    }
}
