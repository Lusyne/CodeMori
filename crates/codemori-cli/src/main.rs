use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand, error::ErrorKind};
use codemori_core::PROTOCOL_VERSION;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "codemori",
    version,
    about = "Keep your knowledge close to your code"
)]
struct Arguments {
    /// Override ~/.codemori/ (useful for isolated development and tests).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print runtime information as JSON, without creating files.
    Info,
    /// Initialize the local SQLite storage baseline.
    Init,
    /// Execute a protocol-1 JSON request from stdin (see docs/cli-protocol.md).
    Rpc,
}

fn failure(code: &str, message: String) -> Value {
    json!({"protocol_version": PROTOCOL_VERSION, "ok": false,
        "error": {"code": code, "message": message}})
}

fn output(value: &Value, status: u8) -> ExitCode {
    let mut stdout = io::stdout().lock();
    if writeln!(stdout, "{value}").is_err() {
        eprintln!("Cannot write the JSON response to stdout");
        return ExitCode::FAILURE;
    }
    ExitCode::from(status)
}

fn main() -> ExitCode {
    let args = match Arguments::try_parse() {
        Ok(args) => args,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                return match error.print() {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(_) => ExitCode::FAILURE,
                };
            }
            return output(&failure("INVALID_ARGUMENT", error.to_string()), 2);
        }
    };
    let result = run(args);
    match result {
        Ok(data) => output(
            &json!({"protocol_version": PROTOCOL_VERSION, "ok": true, "data": data}),
            0,
        ),
        Err(error) => output(&failure(error.code(), error.to_string()), 1),
    }
}

fn run(args: Arguments) -> Result<Value, codemori_core::Error> {
    let info = codemori_core::runtime_info(args.data_dir)?;
    match args.command {
        Command::Info => Ok(serde_json::to_value(info)?),
        Command::Init => {
            codemori_core::initialize(&info)?;
            Ok(serde_json::to_value(info)?)
        }
        Command::Rpc => {
            const MAX_BYTES: u64 = 128 * 1024 * 1024;
            let mut data = Vec::new();
            io::stdin().take(MAX_BYTES + 1).read_to_end(&mut data)?;
            if data.len() as u64 > MAX_BYTES {
                return Err(codemori_core::Error::Validation(
                    "RPC payload exceeds 128 MiB".into(),
                ));
            }
            let request = serde_json::from_slice(&data)?;
            codemori_core::rpc::execute(&info, request)
        }
    }
}
