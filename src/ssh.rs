use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use russh::{client, ChannelMsg, Disconnect};
use russh_keys::key::{KeyPair, PublicKey};
use tokio::time::timeout;

use crate::cli::RemoteHost;

pub async fn connect(remote_host: &RemoteHost, key_pair: &KeyPair) -> Result<Session> {
    let ssh = Session::connect(remote_host, key_pair.clone()).await?;

    Ok(ssh)
}

struct Client {}

#[async_trait]
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct Session {
    session: client::Handle<Client>,
}

impl Session {
    async fn connect(remote_host: &RemoteHost, key_pair: KeyPair) -> Result<Self> {
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(5)),
            ..<_>::default()
        };

        let config = Arc::new(config);
        let sh = Client {};

        let host = remote_host.host.clone();
        let username = remote_host.username.clone();
        let addr = (host, remote_host.port);
        let timeout_duration = Duration::from_secs(5);

        let mut session = timeout(timeout_duration, client::connect(config, addr, sh)).await??;

        let auth_res = session
            .authenticate_publickey(username, Arc::new(key_pair))
            .await?;

        if !auth_res {
            anyhow::bail!("Authentication failed");
        }

        Ok(Self { session })
    }

    pub async fn call(
        &self,
        command: &str,
        sudo: &Option<String>,
        sudo_password: &Option<String>,
    ) -> Result<(String, String, Option<u32>, u64)> {
        let mut channel = self.session.channel_open_session().await?;
        let start_time = std::time::Instant::now();

        match (sudo, sudo_password) {
            (Some(sudo), Some(pass)) => {
                let command = format!("echo {} | sudo -u {} -S  {}", pass, sudo, command);
                channel.exec(true, command.as_str()).await?;
            }
            (Some(sudo), None) => {
                let command = format!("sudo -u {} {}", sudo, command);
                channel.exec(true, command.as_str()).await?;
            }
            _ => {
                channel.exec(true, command).await?;
            }
        }

        let mut code = None;
        let mut out = String::new();
        let mut err = String::new();

        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Data { ref data } => {
                    let data = std::str::from_utf8(data)?.trim();
                    out.push_str(data);
                }

                ChannelMsg::ExtendedData { ref data, .. } => {
                    let data = std::str::from_utf8(data)?.trim();
                    err.push_str(data);
                }

                ChannelMsg::ExitStatus { exit_status } => {
                    code = Some(exit_status);
                }
                _ => {}
            }
        }
        let elapsed = start_time.elapsed();
        let duration: u64 = elapsed.as_secs() * 1000 + u64::from(elapsed.subsec_millis());

        code.ok_or_else(|| anyhow::anyhow!("No exit code"))?;
        Ok((out, err, code, duration))
    }

    pub async fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

#[derive(clap::Parser)]
pub struct Cli {
    #[clap(index = 1)]
    host: String,

    #[clap(long, short, default_value_t = 22)]
    port: u16,

    #[clap(long, short)]
    username: Option<String>,

    #[clap(long, short = 'k')]
    private_key: PathBuf,
}
