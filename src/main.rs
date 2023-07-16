mod args;
mod run;

use std::path::PathBuf;
use vinezombie::client::{
    auth::{AnySasl, Clear},
    conn::ServerAddr,
    nick::Suffix,
    register::{self, BotDefaults},
    tls::{TlsConfig, Trust},
    Queue,
};

type Register = register::Register<Clear, AnySasl<Clear>, Suffix>;

fn parse_register(path: PathBuf) -> std::io::Result<Register> {
    let read = std::fs::File::open(path)?;
    serde_yaml::from_reader(read)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn main() {
    use clap::Parser;
    let args = args::Args::parse();
    let level = if args.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO };
    tracing_subscriber::fmt().with_max_level(level).compact().init();
    if let Err(e) = main_fal(args) {
        tracing::error!("{}", e);
        std::process::exit(1);
    }
}

pub fn main_fal(args: args::Args) -> Result<(), String> {
    let sa = vinezombie::client::conn::ServerAddr {
        address: args.address.try_into().unwrap(),
        tls: args.tls,
        port: args.port,
    };
    let tls = TlsConfig {
        trust: if args.tls_noverify { Trust::NoVerify } else { Trust::Default },
        cert: args.client_cert,
    };
    let cfg = if let Some(cfg) = args.register.map(parse_register) {
        Some(cfg.map_err(|e| format!("invalid registration config: {e}"))?)
    } else {
        None
    };
    let mut readline =
        rustyline::DefaultEditor::new().map_err(|e| format!("cannot init rustyline: {e}"))?;
    let (send, recv) = tokio::sync::mpsc::unbounded_channel();
    let thread = run::run(recv, sa, tls, cfg, args.strict);
    loop {
        match readline.readline("") {
            Ok(line) => {
                let _ = readline.add_history_entry(line.as_str());
                if let Err(_) = send.send(line) {
                    break;
                }
            }
            Err(_) => {
                break;
            }
        }
    }
    std::mem::drop(send);
    thread.join().unwrap();
    Ok(())
}
