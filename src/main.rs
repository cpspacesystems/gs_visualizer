use gs_visualizer::{Bridge, BridgeError, load_config};
use std::{env, ffi::OsString, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(err) = run().await {
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

async fn run() -> Result<(), BridgeError> {
    let config_path = config_path_from_args()?;
    let config = load_config(&config_path)?;
    let bridge = Bridge::from_config(config)?;
    bridge.run_until_shutdown().await
}

fn config_path_from_args() -> Result<OsString, BridgeError> {
    let mut args = env::args_os();
    let _binary = args.next();

    match (args.next(), args.next()) {
        (Some(path), None) => Ok(path),
        _ => Err(BridgeError::Argument(
            "usage: gs_visualizer <path/to/config.toml>".to_string(),
        )),
    }
}
