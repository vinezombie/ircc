mod args;
mod run;

use args::Command;
use run::Io;
use rustyline::error::ReadlineError;
use vinezombie::client::{
    auth::{AnySasl, Clear},
    nick::Suffix,
};

type Register = vinezombie::client::register::Register<Clear, AnySasl<Clear>, Suffix>;

fn main() {
    use clap::Parser;
    let args = args::Args::parse();
    let level = if args.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO };
    tracing_subscriber::fmt().with_max_level(level).compact().init();
    if let Err(e) = main_fal(args) {
        if !e.is_empty() {
            tracing::error!("{}", e);
        }
        std::process::exit(1);
    }
}

pub fn main_fal(args: args::Args) -> Result<(), String> {
    use std::sync::{Arc, Barrier, OnceLock};
    let mut readline =
        rustyline::DefaultEditor::new().map_err(|e| format!("cannot init rustyline: {e}"))?;
    if let Some(history) = &args.history {
        if let Err(e) = readline.load_history(history) {
            tracing::info!("did not load history: {e}");
        }
    }
    let (send, recv) = tokio::sync::mpsc::unbounded_channel();
    let (mut io, conn) = match args.cmd {
        Command::Raw { register } => {
            let io = Io {
                input: recv,
                output: run::Output::Stdio(tokio::io::stdout()),
                in_fn: run::InFn::Raw,
                out_fn: run::OutFn::Raw,
            };
            let conn = run::Connect::new(args.conn, register).map_err(|e| e.to_string())?;
            (io, conn)
        }
    };
    let cell = Arc::new(OnceLock::new());
    let barrier = Arc::new(Barrier::new(2));
    let cell2 = cell.clone();
    let barrier2 = barrier.clone();
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut waited = false;
        let _ = cell2.set(runtime.block_on(async {
            if let Err(e) = io.run(&conn, &barrier2, &mut waited).await {
                tracing::error!("{e}");
                Err(String::new())
            } else {
                Ok(())
            }
        }));
        if !waited {
            barrier2.wait();
        }
    });
    barrier.wait();
    if cell.get().is_none() {
        tracing::info!("ready for input");
        loop {
            match readline.readline("") {
                Ok(line) => {
                    let _ = readline.add_history_entry(line.as_str());
                    if send.send(line.into_bytes()).is_err() {
                        break;
                    }
                }
                Err(ReadlineError::WindowResized) => (),
                Err(_) => {
                    break;
                }
            }
            if cell.get().is_some() {
                break;
            }
        }
    }
    if let Some(history) = &args.history {
        if let Err(e) = readline.append_history(history) {
            tracing::warn!("did not save history: {e}");
        }
    }
    std::mem::drop(send);
    thread.join().unwrap();
    Arc::try_unwrap(cell).unwrap().take().unwrap()
}
