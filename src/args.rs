use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct ConnOptions {
    /// Use TLS
    #[clap(short = 't', long)]
    pub tls: bool,
    /// Skip server identity verification
    #[clap(short = 'T', long)]
    pub tls_noverify: bool,
    /// PEM file containing TLS client certificate
    #[clap(short = 'C', long)]
    pub client_cert: Option<PathBuf>,
    /// The port number to use
    #[clap(short = 'p', long)]
    pub port: Option<u16>,
    /// The address of the IRC server to connect to
    pub address: String,
}

#[derive(clap::Parser)]
pub struct Args {
    /// Abort if the client tries to send an invalid message
    #[clap(short = 's', long)]
    pub strict: bool,
    /// The file to use for command history
    #[clap(short = 'H', long)]
    pub history: Option<PathBuf>,
    /// Log messages at the debug level
    #[clap(short = 'v')]
    pub verbose: bool,
    #[clap(flatten)]
    pub conn: ConnOptions,
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(clap::Parser)]
pub enum Command {
    /// Exchange raw IRC messages between the server and stdin+stdout
    Raw {
        /// Do connection registration using options in this YAML file
        register: Option<PathBuf>,
    },
}

impl Default for Command {
    fn default() -> Self {
        Command::Raw { register: None }
    }
}
