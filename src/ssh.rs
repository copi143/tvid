use anyhow::Result;
use parking_lot::Mutex;
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use crate::TOKIO_RUNTIME;
use crate::config;

struct Server;

impl russh::server::Server for Server {
    type Handler = Handler;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        Handler
    }
}

struct Handler;

static SESSIONS: Mutex<Vec<&mut Session>> = Mutex::new(Vec::new());

impl russh::server::Handler for Handler {
    type Error = anyhow::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool> {
        session.channel_success(channel.id())?;
        Ok(true)
    }

    async fn data(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<()> {
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        session: &mut Session,
    ) -> Result<()> {
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
