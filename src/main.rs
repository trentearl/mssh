mod cli;

mod run;
mod ssh;

use tabled::{settings::Style, Table, Tabled};

use thiserror::Error;

use anyhow::Result;
use cli::{cli, Output, RemoteHost};
use console::Term;
use run::run;
use serde::Serialize;

#[derive(Debug, Error, Serialize)]
pub enum RunError {
    #[error("An error occurred: {0}")]
    GeneralError(String),

    #[error("SSH Connection error: {0}")]
    SshConnectionError(String),

    #[error("SSH Run error: {0}")]
    SshRunError(String, usize),

    #[error("SSH error occurred: {0}")]
    SshCloseError(String),
}

pub type RunResult<T> = Result<T, RunError>;

#[derive(Debug, Serialize)]
pub struct Response {
    pub index: usize,
    pub out: String,
    pub err: String,
    pub code: Option<u32>,
    pub duration: u64,
}

#[derive(Debug, Serialize)]
pub struct Responses {
    pub remote_host: RemoteHost,
    pub responses: Vec<RunResult<Response>>,
}

#[derive(Debug, Serialize, Tabled)]
pub struct CompactResponse {
    pub remote_host: RemoteHost,
    pub out: String,

    #[tabled(display_with = "duration_print")]
    pub duration: u64,

    #[tabled(display_with = "success_print")]
    pub success: bool,
}

fn success_print(success: &bool) -> String {
    let emoji = match success {
        true => emojis::get("✅"),
        false => emojis::get("❌"),
    };

    match emoji {
        Some(emoji) => emoji.to_string(),
        None => success.to_string(),
    }
}

fn duration_print(duration: &u64) -> String {
    format!("{}ms", duration)
}

impl CompactResponse {
    pub fn new(
        remote_host: RemoteHost,
        output_or_error: String,
        success: bool,
        duration: u64,
    ) -> Self {
        Self {
            remote_host,
            out: output_or_error,
            success,
            duration,
        }
    }
}

impl From<&Responses> for Vec<CompactResponse> {
    fn from(responses: &Responses) -> Self {
        let mut compact_responses = vec![];

        for response in &responses.responses {
            match response {
                Ok(res) => {
                    compact_responses.push(CompactResponse::new(
                        responses.remote_host.clone(),
                        res.out.clone(),
                        true,
                        res.duration,
                    ));
                }
                Err(e) => {
                    compact_responses.push(CompactResponse::new(
                        responses.remote_host.clone(),
                        e.to_string(),
                        false,
                        0,
                    ));
                }
            }
        }

        compact_responses
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = cli()?;
    let output = args.output.clone();
    let term = Term::stdout();
    let mut all_responses = run(args).await?;
    all_responses.sort_by(|a, b| a.remote_host.host.cmp(&b.remote_host.host));

    match output {
        Output::Json => {
            let compact_responses: Vec<CompactResponse> = all_responses
                .iter()
                .flat_map(|responses| {
                    let l: Vec<CompactResponse> = responses.into();
                    l
                })
                .collect();

            let json = serde_json::to_string_pretty(&compact_responses)?;
            term.write_line(json.as_str())?;
        }
        Output::Text => {
            for responses in all_responses {
                for response in responses.responses {
                    match response {
                        Ok(res) => {
                            let line = format!("{:>15}: {}", responses.remote_host.host, res.out);
                            term.write_line(line.as_str())?;
                        }
                        Err(e) => {
                            let line = format!("{:>15}: {}", responses.remote_host.host, e);
                            term.write_line(line.as_str())?;
                        }
                    }
                }
            }
        }
        Output::Table => {
            let compact_responses: Vec<CompactResponse> = all_responses
                .iter()
                .flat_map(|responses| {
                    let l: Vec<CompactResponse> = responses.into();
                    l
                })
                .collect();

            let table = Table::new(compact_responses)
                .with(Style::modern_rounded())
                .to_string();

            term.write_line(table.as_str())?;
        }
    }

    Ok(())
}
