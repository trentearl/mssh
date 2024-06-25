use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use dirs::home_dir;

use anyhow::Result;
use russh_keys::key::KeyPair;
use russh_keys::load_secret_key;
use serde::{Serialize, Serializer};

#[derive(clap::Parser)]
struct Cli {
    #[clap(long, short = 'c', alias = "command")]
    commands: Vec<String>,

    #[clap(long, short = 'k')]
    private_key: Option<PathBuf>,

    #[clap(long, short = 'p')]
    sudo_prompt_password: bool,

    #[arg(num_args=1..)]
    #[clap(value_parser = parse_host_login)]
    hosts: Vec<RemoteHost>,

    #[clap(long, short, default_value = "table")]
    output: Output,
}

#[derive(ValueEnum, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum Output {
    Json,
    Text,
    Table,
}

pub struct Args {
    pub commands: Vec<String>,
    pub key_pair: KeyPair,
    pub hosts: Vec<RemoteHost>,
    pub output: Output,
    pub sudo_prompt_password: bool,
}

pub fn cli() -> Result<Args> {
    let cli = Cli::parse();

    let home = home_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
    let private_key_path = cli.private_key.unwrap_or_else(|| {
        let mut path = home;
        path.push(".ssh/id_ed25519");
        path
    });

    let key_pair = load_secret_key(private_key_path, None)?;
    Ok(Args {
        sudo_prompt_password: cli.sudo_prompt_password,
        commands: cli.commands,
        key_pair,
        hosts: cli.hosts,
        output: cli.output,
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoteHost {
    pub host: String,
    pub sudo: Option<String>,
    pub username: String,
    pub port: u16,
}

impl std::fmt::Display for RemoteHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let user = self.sudo.as_deref().unwrap_or(self.username.as_str());
        write!(f, "{}@{}", user, self.host,)
    }
}

impl Serialize for RemoteHost {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = self.to_string();
        serializer.serialize_str(&str)
    }
}

fn parse_users(s: &str) -> Vec<String> {
    let mut split = s.split('@').collect::<Vec<_>>();
    split.pop();

    split.iter().map(|s| s.to_string()).collect()
}

fn parse_host_login(host_login: &str) -> Result<RemoteHost> {
    let (username, sudo) = match parse_users(host_login).as_slice() {
        [username] => (username.to_string(), None),
        [username, sudo] => (username.to_string(), Some(sudo.to_string())),
        [] => (std::env::var("USER")?, None),
        _ => return Err(anyhow::anyhow!("Invalid user name")),
    };

    let host_string = match host_login.split('@').collect::<Vec<_>>().pop() {
        Some(s) => s,
        None => return Err(anyhow::anyhow!("No host provided")),
    };

    let parts: Vec<_> = host_string.split(':').collect();

    let (host, port) = match parts.as_slice() {
        [host] => (host, 22),
        [host, port] => (host, port.parse()?),
        _ => return Err(anyhow::anyhow!("Invalid host")),
    };

    if host.is_empty() {
        return Err(anyhow::anyhow!("Invalid host"));
    }

    Ok(RemoteHost {
        host: host.to_string(),
        sudo,
        username,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_users_single_user() {
        let input = "user@";
        let expected = vec!["user".to_string()];
        let result = parse_users(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_users_multiple_users() {
        let input = "user@sudo@";
        let expected = vec!["user".to_string(), "sudo".to_string()];
        let result = parse_users(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_users_no_users() {
        let input = "";
        let expected: Vec<String> = Vec::new();
        let result = parse_users(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_host_login_valid_single_user() {
        let input = "user@host";
        let expected = RemoteHost {
            username: "user".to_string(),
            sudo: None,
            host: "host".to_string(),
            port: 22,
        };
        let result = parse_host_login(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_host_login_valid_multiple_users() {
        let input = "user@sudo@host";
        let expected = RemoteHost {
            username: "user".to_string(),
            sudo: Some("sudo".to_string()),
            host: "host".to_string(),
            port: 22,
        };
        let result = parse_host_login(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_host_login_with_port() {
        let input = "user@host:2222";
        let expected = RemoteHost {
            username: "user".to_string(),
            sudo: None,
            host: "host".to_string(),
            port: 2222,
        };
        let result = parse_host_login(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_host_login_invalid_user() {
        let input = "user@sudo@extra@host";
        let result = parse_host_login(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_login_no_host() {
        let input = "user@";
        let result = parse_host_login(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_login_env_user() {
        std::env::set_var("USER", "env_user");
        let input = "host";
        let expected = RemoteHost {
            username: "env_user".to_string(),
            sudo: None,
            host: "host".to_string(),
            port: 22,
        };
        let result = parse_host_login(input).unwrap();
        assert_eq!(result, expected);
    }
}
