use std::path::PathBuf;
use vinezombie::client::{
    auth::{AnySasl, Clear},
    conn::ServerAddr,
    nick::SuffixRandom,
    register::{self, BotDefaults},
    tls::{TlsConfig, Trust},
    Queue,
};

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
    /// Log messages at the debug level
    #[clap(short = 'v')]
    pub verbose: bool,
    /// The address of the IRC server to connect to
    pub address: String,
}

type Register = register::Register<Clear, AnySasl<Clear>, SuffixRandom>;

fn parse_register(path: PathBuf) -> std::io::Result<Register> {
    let read = std::fs::File::open(path)?;
    serde_yaml::from_reader(read)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn main() {
    use clap::Parser;
    let args = Args::parse();
    let level = if args.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO };
    tracing_subscriber::fmt().with_max_level(level).compact().init();
    let sa = vinezombie::client::conn::ServerAddr {
        address: args.address.try_into().unwrap(),
        tls: args.tls,
        port: args.port,
    };
    let tls = TlsConfig {
        trust: if args.tls_noverify { Trust::NoVerify } else { Trust::Default },
        cert: args.client_cert,
    };
    let cfg = match args.register.map(parse_register) {
        Some(Ok(cfg)) => Some(cfg),
        Some(Err(e)) => {
            tracing::error!("invalid registration config: {}", e);
            std::process::exit(1);
        }
        None => None,
    };
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    if let Err(e) = rt.block_on(main_async(sa, tls, cfg, args.strict)) {
        tracing::error!("{}", e);
        std::process::exit(1);
    }
}

async fn main_async(
    sa: ServerAddr<'static>,
    tls: TlsConfig,
    cfg: Option<Register>,
    strict: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use vinezombie::ircmsg::{ClientMsg, ServerMsg};
    let tls = tls.build()?;
    let mut conn = sa.connect_tokio(tls).await?;
    let mut queue = Queue::new();
    if let Some(cfg) = &cfg {
        let mut handler = cfg.handler(&BotDefaults, &mut queue)?;
        vinezombie::client::run_handler_tokio(&mut conn, &mut queue, &mut handler).await?;
    }
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    let mut buf_i = Vec::with_capacity(512);
    let mut buf_o = Vec::with_capacity(512);
    let mut timeout = Option::<std::time::Duration>::None;
    let mut read_stdin = true;
    loop {
        if let Some(msg_o) = queue.pop(|dur| timeout = dur) {
            msg_o.send_to_tokio(&mut conn, &mut buf_o).await?;
            continue;
        }
        tokio::select! {
            line = stdin.next_line(), if read_stdin => {
                let Ok(Some(mut line)) = line else {
                    tracing::debug!("end of stdin");
                    read_stdin = false;
                    continue;
                };
                line.pop();
                if line.is_empty() {
                    continue;
                }
                match ClientMsg::parse(line) {
                    Ok(msg_o) => queue.push(msg_o),
                    Err(e) => if strict {
                        return Err(e.into());
                    } else {
                        tracing::error!("invalid client msg: {}", e);
                    }
                }
            },
            msg_i = ServerMsg::read_owning_from_tokio(&mut conn, &mut buf_i) => {
                let msg_i = msg_i?;
                if let Some(pong) = vinezombie::client::pong(&msg_i) {
                    queue.push(pong.owning());
                    continue;
                }
                msg_i.write_to(&mut buf_o)?;
                buf_o.push(b'\n');
                stdout.write_all(&buf_o).await?;
                buf_o.clear();
                if msg_i.kind == "ERROR" {
                    return Ok(());
                }
            },
            () = tokio::time::sleep(timeout.unwrap_or_default()), if timeout.is_some() => ()
        }
    }
}
