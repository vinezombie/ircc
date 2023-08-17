use crate::Register;
use std::{
    path::Path,
    sync::{Arc, Barrier},
};
use tokio::sync::mpsc::UnboundedReceiver;
use vinezombie::{
    client::{
        register::BotDefaults,
        tls::{TlsConfig, TlsConfigOptions, Trust},
        Queue,
    },
    consts::cmd::QUIT,
    error::InvalidString,
    ircmsg::{ClientMsg, Numeric, ServerMsg, SharedSource},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid address: {0}")]
    InvalidAddress(InvalidString),
    #[error("cannot load register.yml: {0}")]
    NoRegister(std::io::Error),
    #[error("invalid register.yml: {0}")]
    InvalidRegister(serde_yaml::Error),
    #[error("invalid client msg: {0}")]
    InvalidMessage(anyhow::Error),
    #[error("no tls support: {0}")]
    NoTls(Arc<std::io::Error>),
    #[error("io error: {0}")]
    Io(std::io::Error),
    #[error("irc handshake failure: {0}")]
    Register(vinezombie::client::register::HandlerError),
}

pub struct Connect {
    sa: vinezombie::client::conn::ServerAddr<'static>,
    tls: Result<TlsConfig, Arc<std::io::Error>>,
    cfg: Option<Register>,
}

impl Connect {
    pub fn new(
        opts: crate::args::ConnOptions,
        register: Option<impl AsRef<Path>>,
    ) -> Result<Self, Error> {
        let sa = vinezombie::client::conn::ServerAddr {
            address: opts.address.try_into().map_err(Error::InvalidAddress)?,
            tls: opts.tls,
            port: opts.port,
        };
        let cfg = if let Some(cfg) = register {
            let reader = std::fs::File::open(cfg).map_err(Error::NoRegister)?;
            let parsed = serde_yaml::from_reader(reader).map_err(Error::InvalidRegister)?;
            Some(parsed)
        } else {
            None
        };
        let tls = TlsConfigOptions {
            trust: if opts.tls_noverify { Trust::NoVerify } else { Trust::Default },
            cert: opts.client_cert,
        };
        let tls = tls.build().map_err(Arc::new);
        Ok(Connect { sa, tls, cfg })
    }
}

pub enum Output {
    Stdio(tokio::io::Stdout),
}

impl Output {
    pub async fn send(&mut self, string: &mut Vec<u8>) -> std::io::Result<()> {
        match self {
            Output::Stdio(so) => {
                use tokio::io::AsyncWriteExt;
                let retval = so.write_all(string).await;
                string.clear();
                retval
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum InFn {
    Raw,
}

impl InFn {
    pub fn parse(&self, msg: Vec<u8>, queue: &mut Queue<'static>) -> Result<(), anyhow::Error> {
        match self {
            InFn::Raw => {
                let msg = ClientMsg::parse(msg)?;
                queue.push(msg);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum OutFn {
    Raw,
}

impl OutFn {
    pub fn write(&self, msg: &ServerMsg<'static>, buf: &mut Vec<u8>) {
        msg.write_to(buf).unwrap();
        buf.push(b'\n');
    }
}

pub struct Io {
    pub input: UnboundedReceiver<Vec<u8>>,
    pub output: Output,
    pub in_fn: InFn,
    pub out_fn: OutFn,
}

impl Io {
    pub async fn run(
        &mut self,
        cfg: &Connect,
        barrier: &Barrier,
        waited: &mut bool,
    ) -> Result<(), Error> {
        let mut conn = if cfg.sa.tls {
            let tls = cfg.tls.clone().map_err(Error::NoTls)?;
            cfg.sa.connect_tokio(|| Ok(tls)).await
        } else {
            cfg.sa.connect_tokio_no_tls().await
        }
        .map_err(Error::Io)?;
        let mut queue = Queue::new();
        let mut buf_o = Vec::with_capacity(512);
        if let Some(cfg) = &cfg.cfg {
            tracing::info!("registering connection");
            let mut handler = cfg.handler(&BotDefaults, &mut queue).map_err(Error::Io)?;
            let reg = vinezombie::client::run_handler_tokio(&mut conn, &mut queue, &mut handler)
                .await
                .map_err(Error::Register)?;
            let welcome = ServerMsg {
                tags: Default::default(),
                source: reg.source.map(SharedSource::new),
                kind: Numeric::from_int(1).unwrap().into(),
                args: reg.welcome,
            };
            // This should be a pretty short block.
            barrier.wait();
            *waited = true;
            self.out_fn.write(&welcome, &mut buf_o);
            self.output.send(&mut buf_o).await.map_err(Error::Io)?;
        } else {
            barrier.wait();
            *waited = true;
        }
        let mut buf_i = Vec::with_capacity(512);
        let mut timeout = Option::<std::time::Duration>::None;
        let mut read_stdin = true;
        loop {
            if let Some(msg_o) = queue.pop(|dur| timeout = dur) {
                msg_o.send_to_tokio(&mut conn, &mut buf_o).await.map_err(Error::Io)?;
                continue;
            }
            tokio::select! {
                line = self.input.recv(), if read_stdin => {
                    let Some(mut line) = line else {
                        read_stdin = false;
                        queue.push(ClientMsg::new_cmd(QUIT));
                        continue;
                    };
                    while line.last().is_some_and(u8::is_ascii_whitespace) {
                        line.pop();
                    }
                    if line.is_empty() {
                        continue;
                    }
                    if let Err(e) = self.in_fn.parse(line, &mut queue) {
                        // TODO: Strict.
                        tracing::error!("{}", Error::InvalidMessage(e));
                    }
                },
                msg_i = ServerMsg::read_owning_from_tokio(&mut conn, &mut buf_i) => {
                    let msg_i = msg_i.map_err(Error::Io)?;
                    if let Some(pong) = vinezombie::client::pong(&msg_i) {
                        queue.push(pong.owning());
                        continue;
                    }
                    self.out_fn.write(&msg_i, &mut buf_o);
                    self.output.send(&mut buf_o).await.map_err(Error::Io)?;
                    if msg_i.kind == "ERROR" {
                        return Ok(());
                    }
                },
                () = tokio::time::sleep(timeout.unwrap_or_default()), if timeout.is_some() => ()
            }
        }
    }
}
