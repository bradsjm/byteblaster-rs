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

#[derive(Debug, Subcommand)]
enum Commands {
    Stream {
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
}

#[derive(Debug, Parser)]
#[command(name = "byteblaster")]
#[command(about = "ByteBlaster console client")]
struct Cli {
    #[arg(long, value_enum, default_value_t = FormatArg::Text)]
    format: FormatArg,
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

    match cli.command {
        Commands::Stream {
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
            cmd::stream::run(format, input, live).await
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
            cmd::download::run(format, output_dir, input, live).await
        }
        Commands::Inspect { input } => cmd::inspect::run(format, input).await,
    }
}
