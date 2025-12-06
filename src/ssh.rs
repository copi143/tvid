use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use russh::server::{Auth, Handle, Msg, Server as _, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodKind, MethodSet, Pty, Sig};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TryRecvError;

use crate::TOKIO_RUNTIME;
use crate::config;
use crate::stdin::input_task;
use crate::term::{TERM_EXIT_SEQ, TERM_INIT_SEQ, Winsize};

pub static TERMINALS: Mutex<BTreeMap<i32, Arc<Terminal>>> = Mutex::new(BTreeMap::new());

pub struct Terminal {
    id: i32,
    tx: Sender<u8>,
    channel: ChannelId,
    session: Handle,
    winsize: Mutex<Winsize>,
}

impl Terminal {
    async fn new(channel: ChannelId, session: &mut Session) -> Arc<Self> {
        let id = crate::term::next_term_id();
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        tokio::spawn(input_task(
            id,
            Box::new(move || match rx.try_recv() {
                Ok(c) => Ok(Some(c)),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("Channel disconnected")),
            }),
        ));
        let term = Arc::new(Self {
            id,
            tx,
            channel,
            session: session.handle(),
            winsize: Mutex::new(Winsize {
                row: 24,
                col: 80,
                xpixel: 0,
                ypixel: 0,
            }),
        });
        TERMINALS.lock().insert(id, term.clone());
        term
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn winsize(&self) -> Winsize {
        *self.winsize.lock()
    }

    pub async fn resize(&self, col: u16, row: u16, xpixel: u16, ypixel: u16) {
        let mut lock = self.winsize.lock();
        lock.col = col;
        lock.row = row;
        lock.xpixel = xpixel;
        lock.ypixel = ypixel;
    }

    pub async fn stdin_byte(&self, data: u8) -> Result<()> {
        if let Err(e) = self.tx.send(data).await {
            bail!("Failed to send byte to input task: {e}");
        }
        Ok(())
    }

    pub async fn stdin(&self, data: &[u8]) -> Result<()> {
        for &byte in data {
            if let Err(e) = self.tx.send(byte).await {
                bail!("Failed to send byte to input task: {e}");
            }
        }
        Ok(())
    }

    pub async fn stdout_byte(&self, data: u8) -> Result<()> {
        self.session
            .data(self.channel, CryptoVec::from_slice(&[data]))
            .await
            .ok()
            .context("Failed to send data to SSH client")?;
        Ok(())
    }

    pub async fn stdout(&self, data: &[u8]) -> Result<()> {
        self.session
            .data(self.channel, CryptoVec::from_slice(data))
            .await
            .ok()
            .context("Failed to send data to SSH client")?;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        self.session
            .close(self.channel)
            .await
            .ok()
            .context("Failed to close SSH channel")?;
        let id = self.id;
        tokio::spawn(async move { TERMINALS.lock().remove(&id) });
        Ok(())
    }
}

struct Server;

impl russh::server::Server for Server {
    type Handler = Handler;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        Handler::new()
    }
}

static NEXT_CONN_ID: AtomicI32 = AtomicI32::new(1);

struct Handler {
    id: i32,
    channels: BTreeMap<ChannelId, Arc<Terminal>>,
}

impl Handler {
    pub fn new() -> Self {
        Self {
            id: NEXT_CONN_ID.fetch_add(1, Ordering::SeqCst),
            channels: BTreeMap::new(),
        }
    }
}

impl russh::server::Handler for Handler {
    type Error = anyhow::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth> {
        Ok(Auth::Accept)
    }

    async fn channel_close(&mut self, channel: ChannelId, session: &mut Session) -> Result<()> {
        info!("Channel {channel} closed by client {}", self.id);
        self.channels.remove(&channel);
        Ok(())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool> {
        info!("Channel {} opened by client {}", channel.id(), self.id);
        Ok(true)
    }

    #[rustfmt::skip]
    async fn data(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<()> {
        for &byte in data {
            if byte == 0x03 {
                info!("Received Ctrl-C on channel {channel} from client {}", self.id);
                session.data(channel, CryptoVec::from_slice(TERM_EXIT_SEQ))?;
                session.data(channel, CryptoVec::from_slice(b"Disconnecting from tvid SSH session...\r\n"))?;
                session.data(channel, CryptoVec::from_slice(b"Bye!\r\n"))?;
                session.close(channel)?;
                break;
            } else {
                let Some(term) = self.channels.get(&channel) else {
                    break;
                };
                if let Err(e) = term.stdin_byte(byte).await {
                    error!("Failed to send byte to input task: {e}");
                    break;
                }
            }
        }
        Ok(())
    }

    #[rustfmt::skip]
    async fn pty_request(&mut self, channel: ChannelId, term: &str, col_width: u32, row_height: u32, pix_width: u32, pix_height: u32, modes: &[(Pty, u32)], session: &mut Session) -> Result<()> {
        let term = Terminal::new(channel, session).await;
        term.resize(col_width as u16, row_height as u16, pix_width as u16, pix_height as u16).await;
        self.channels.insert(channel, term);
        session.channel_success(channel)?;
        session.data(channel, CryptoVec::from_slice(b"PTY request accepted\r\n"))?;
        session.data(channel, CryptoVec::from_slice(b"Welcome to tvid SSH session!\r\n"))?;
        session.data(channel, CryptoVec::from_slice(TERM_INIT_SEQ))?;
        Ok(())
    }

    // async fn env_request(
    //     &mut self,
    //     channel: ChannelId,
    //     variable_name: &str,
    //     variable_value: &str,
    //     session: &mut Session,
    // ) -> Result<()> {
    //     info!(
    //         "Env request on channel {}: {}={}",
    //         channel, variable_name, variable_value
    //     );
    //     session.channel_success(channel)?;
    //     Ok(())
    // }

    // async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<()> {
    //     session.channel_success(channel)?;
    //     Ok(())
    // }

    // async fn exec_request(
    //     &mut self,
    //     channel: ChannelId,
    //     data: &[u8],
    //     session: &mut Session,
    // ) -> Result<()> {
    //     let command = String::from_utf8_lossy(data);
    //     info!("Exec request on channel {}: {}", channel, command);
    //     session.channel_success(channel)?;
    //     Ok(())
    // }

    #[rustfmt::skip]
    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        session: &mut Session,
    ) -> Result<()> {
        if let Some(term) = self.channels.get(&channel) {
            term.resize(col_width as u16, row_height as u16, pix_width as u16, pix_height as u16).await;
        }
        session.channel_success(channel)?;
        Ok(())
    }

    async fn signal(
        &mut self,
        channel: ChannelId,
        signal: Sig,
        session: &mut Session,
    ) -> Result<()> {
        session.close(channel)?;
        Ok(())
    }
}

pub fn run() -> Result<()> {
    let config = Arc::new(russh::server::Config {
        methods: {
            let mut methods = MethodSet::empty();
            methods.push(MethodKind::None);
            methods
        },
        keys: config::load_or_create_hostkeys(None)?,
        ..Default::default()
    });

    let addrs = vec![
        SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 2222),
        SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 2222),
    ];

    TOKIO_RUNTIME.spawn(async move {
        if let Err(e) = Server.run_on_address(config, addrs.as_slice()).await {
            fatal!("SSH server error: {e}");
        }
    });

    info!("SSH server started on port 2222");

    Ok(())
}
