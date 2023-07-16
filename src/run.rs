use crate::Register;
use std::thread::JoinHandle;
use vinezombie::{
    client::{conn::ServerAddr, register::BotDefaults, tls::TlsConfig, Queue},
    ircmsg::Numeric,
};

pub fn run(
    stdin: tokio::sync::mpsc::UnboundedReceiver<String>,
    sa: ServerAddr<'static>,
    tls: TlsConfig,
    cfg: Option<Register>,
    strict: bool,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        if let Err(e) = rt.block_on(main_async(stdin, sa, tls, cfg, strict)) {
            tracing::error!("{}", e);
            std::process::exit(1);
        }
    })
}

async fn main_async(
    mut stdin: tokio::sync::mpsc::UnboundedReceiver<String>,
    sa: ServerAddr<'static>,
    tls: TlsConfig,
    cfg: Option<Register>,
    strict: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::AsyncWriteExt;
    use vinezombie::consts::cmd::QUIT;
    use vinezombie::ircmsg::{ClientMsg, ServerMsg};
    let tls = tls.build()?;
    let mut conn = sa.connect_tokio(tls).await?;
    let mut queue = Queue::new();
    if let Some(cfg) = &cfg {
        tracing::info!("registering connection");
        let mut handler = cfg.handler(&BotDefaults, &mut queue)?;
        let reg =
            vinezombie::client::run_handler_tokio(&mut conn, &mut queue, &mut handler).await?;
        let welcome = ServerMsg {
            tags: Default::default(),
            source: reg.source,
            kind: Numeric::from_int(1).unwrap().into(),
            args: reg.welcome,
        };
        println!("{welcome}");
    }
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
            line = stdin.recv(), if read_stdin => {
                let Some(mut line) = line else {
                    read_stdin = false;
                    queue.push(ClientMsg::new_cmd(QUIT));
                    continue;
                };
                while line.ends_with(char::is_whitespace) {
                    line.pop();
                }
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
