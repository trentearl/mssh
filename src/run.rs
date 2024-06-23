use futures::{stream, StreamExt};
use log::trace;

use crate::cli::Args;
use crate::{ssh, Responses, RunError};
use crate::{Response, RunResult};

pub async fn run(args: Args) -> RunResult<Vec<Responses>> {
    let key_pair = args.key_pair;
    let commands: Vec<String> = args.commands;

    trace!("connecting to {} hosts", args.hosts.len());

    let mut sshs = stream::iter(args.hosts)
        .map(|remote_host| {
            let key_pair = key_pair.clone();
            let commands = commands.clone();

            tokio::spawn(async move {
                let host = &remote_host.host;
                trace!("{:>15}: {:>15}", "Shh connect ", host);
                let mut ssh = match ssh::connect(&remote_host, &key_pair).await {
                    Ok(ssh) => ssh,
                    Err(e) => {
                        return (
                            remote_host,
                            RunResult::Err(RunError::SshConnectionError(e.to_string())),
                        )
                    }
                };
                trace!("{:>15}: {:>15}", "Shh connected ", host);

                let mut responses: Vec<RunResult<Response>> = Vec::new();
                for (i, command) in commands.iter().enumerate() {
                    trace!("{:>15}: {:>15} {}", "Run command", host, command);
                    match ssh.call(command, &remote_host.sudo).await {
                        Ok((out, err, code, duration)) => {
                            responses.push(RunResult::Ok(Response {
                                out,
                                code,
                                err,
                                duration,
                                index: i,
                            }));
                        }
                        Err(e) => {
                            responses.push(RunResult::Err(RunError::SshRunError(e.to_string(), i)));
                            break;
                        }
                    }
                }

                if let Err(e) = ssh.close().await {
                    responses.push(RunResult::Err(RunError::SshCloseError(e.to_string())));
                }

                (remote_host, Ok(responses))
            })
        })
        .buffer_unordered(10);

    let mut ret: Vec<Responses> = vec![];
    while let Some(res) = sshs.next().await {
        match res {
            Ok((remote_host, Ok(responses))) => {
                let mut responses = responses;

                responses.sort_by(|a, b| {
                    let a_index = match a {
                        Ok(a) => Some(a.index),
                        Err(RunError::SshRunError(_, i)) => Some(*i),
                        _ => None,
                    };
                    let b_index = match b {
                        Ok(b) => Some(b.index),
                        Err(RunError::SshRunError(_, i)) => Some(*i),
                        _ => None,
                    };

                    match (a_index, b_index) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        _ => std::cmp::Ordering::Equal,
                    }
                });
                ret.push(Responses {
                    remote_host,
                    responses,
                });
            }

            Ok((remote_host, Err(e))) => {
                ret.push(Responses {
                    remote_host,
                    responses: vec![Err(e)],
                });
            }

            Err(e) => return Err(RunError::GeneralError(e.to_string())),
        }
    }

    Ok(ret)
}
