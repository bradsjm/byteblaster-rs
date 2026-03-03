mod cmd;
mod output;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone)]
struct LiveOptions {
    email: Option<String>,
    servers: Vec<String>,
    server_list_path: Option<String>,
    max_events: usize,
    idle_timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FormatArg {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorArg {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Stream {
        input: Option<String>,
        #[arg(long)]
        output_dir: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long = "server", value_delimiter = ',')]
        servers: Vec<String>,
        #[arg(long)]
        server_list_path: Option<String>,
        #[arg(long)]
        max_events: Option<usize>,
        #[arg(long, default_value_t = 20)]
        idle_timeout_secs: u64,
    },
    Download {
        output_dir: String,
        input: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long = "server", value_delimiter = ',')]
        servers: Vec<String>,
        #[arg(long)]
        server_list_path: Option<String>,
        #[arg(long, default_value_t = 200)]
        max_events: usize,
        #[arg(long, default_value_t = 20)]
        idle_timeout_secs: u64,
    },
    Inspect {
        input: Option<String>,
    },
    Server {
        #[arg(long)]
        email: String,
        #[arg(long = "server", value_delimiter = ',')]
        servers: Vec<String>,
        #[arg(long)]
        server_list_path: Option<String>,
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
        #[arg(long)]
        cors_origin: Option<String>,
        #[arg(long, default_value_t = 100)]
        max_clients: usize,
        #[arg(long, default_value_t = 30)]
        stats_interval_secs: u64,
        #[arg(long, default_value_t = 300)]
        file_retention_secs: u64,
        #[arg(long, default_value_t = 1000)]
        max_retained_files: usize,
        #[arg(long, default_value_t = false)]
        quiet: bool,
    },
}

#[derive(Debug, Parser)]
#[command(name = "byteblaster")]
#[command(about = "ByteBlaster console client")]
struct Cli {
    #[arg(long, value_enum, default_value_t = FormatArg::Text)]
    format: FormatArg,
    #[arg(long, value_enum, default_value_t = ColorArg::Auto)]
    color: ColorArg,
    #[arg(long, default_value_t = false)]
    no_color: bool,
    #[arg(long, default_value_t = 80)]
    text_preview_chars: usize,
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
