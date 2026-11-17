use std::process::ExitCode;

use berth::config::Config;
use berth::ipc::{self, Request};
use berth::switcher::session::Direction;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about = "A dock and window switcher for the COSMIC desktop")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open the window switcher, or move its selection while it is open
    Switch { towards: Towards },
    /// List the windows berth tracks, most recently used first
    Windows,
}

#[derive(Clone, Copy, ValueEnum)]
enum Towards {
    Next,
    Previous,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let outcome = match Cli::parse().command {
        None => Config::load(&Config::path())
            .map_err(anyhow::Error::from)
            .and_then(berth::shell::run),
        Some(Command::Switch {
            towards: Towards::Next,
        }) => ask(Request::Switch(Direction::Next)),
        Some(Command::Switch {
            towards: Towards::Previous,
        }) => ask(Request::Switch(Direction::Previous)),
        Some(Command::Windows) => ask(Request::Windows),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("berth: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn ask(request: Request) -> anyhow::Result<()> {
    let socket = ipc::socket_path();
    let answer = ipc::ask(&socket, request).map_err(|error| {
        anyhow::anyhow!(
            "berth is not running (no answer on {}: {error})",
            socket.display()
        )
    })?;
    match answer.strip_prefix("error: ") {
        Some(problem) => Err(anyhow::anyhow!("{}", problem.trim_end())),
        None => {
            if answer != "ok\n" {
                print!("{answer}");
            }
            Ok(())
        }
    }
}
