use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct Args {
    /// Use TLS
    #[clap(short = 't', long)]
    pub tls: bool,
    /// Skip server identity verification
    #[clap(short = 'T', long)]
    pub tls_noverify: bool,
    /// PEM file containing TLS client certificate
    #[clap(short = 'C', long)]
    pub client_cert: Option<PathBuf>,
    /// Do connection registration using options in this YAML file
    #[clap(short = 'R', long)]
    pub register: Option<PathBuf>,
    /// Abort if the client tries to send an invalid message
    #[clap(short = 's', long)]
    pub strict: bool,
    /// The port number to use
    #[clap(short = 'p', long)]
    pub port: Option<u16>,
    /// The file to use for command history
    #[clap(short = 'H', long)]
    pub history: Option<PathBuf>,
    /// Log messages at the debug level
    #[clap(short = 'v')]
    pub verbose: bool,
    /// The address of the IRC server to connect to
    pub address: String,
}
