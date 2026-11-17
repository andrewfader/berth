use std::env;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use nix::sys::socket::getsockopt;
use nix::sys::socket::sockopt::PeerCredentials;
use nix::unistd::getuid;

use crate::switcher::session::Direction;

const PATIENCE: Duration = Duration::from_millis(500);
const ATTENTION: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Switch(Direction),
    Windows,
    Reload,
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Request::Switch(Direction::Next) => "switch next",
            Request::Switch(Direction::Previous) => "switch previous",
            Request::Windows => "windows",
            Request::Reload => "reload",
        })
    }
}

impl FromStr for Request {
    type Err = String;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        match line.trim() {
            "switch next" => Ok(Request::Switch(Direction::Next)),
            "switch previous" => Ok(Request::Switch(Direction::Previous)),
            "windows" => Ok(Request::Windows),
            "reload" => Ok(Request::Reload),
            other => Err(format!("unknown request: {other}")),
        }
    }
}

pub fn socket_path() -> PathBuf {
    let runtime = env::var_os("XDG_RUNTIME_DIR").map_or_else(env::temp_dir, PathBuf::from);
    let display = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    runtime.join(format!("berth-{display}.sock"))
}

pub fn ask(socket: &Path, request: Request) -> io::Result<String> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(PATIENCE))?;
    writeln!(stream, "{request}")?;
    stream.shutdown(Shutdown::Write)?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    Ok(answer)
}

pub fn listen(socket: &Path) -> io::Result<UnixListener> {
    if UnixStream::connect(socket).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("berth is already running on {}", socket.display()),
        ));
    }
    match fs::remove_file(socket) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub fn answer_pending(listener: &UnixListener, mut answer: impl FnMut(Request) -> String) {
    while let Ok((stream, _)) = listener.accept() {
        if let Err(error) = answer_one(stream, &mut answer) {
            log::warn!("dropped a control connection: {error}");
        }
    }
}

fn answer_one(stream: UnixStream, answer: &mut impl FnMut(Request) -> String) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(ATTENTION))?;
    stream.set_write_timeout(Some(ATTENTION))?;
    if getsockopt(&stream, PeerCredentials)?.uid() != getuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "a request from another user",
        ));
    }
    let mut line = String::new();
    BufReader::new(&stream).take(256).read_line(&mut line)?;
    let reply = match line.parse() {
        Ok(request) => answer(request),
        Err(problem) => format!("error: {problem}\n"),
    };
    (&stream).write_all(reply.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn daemon_answering(socket: &Path, reply: &'static str) -> thread::JoinHandle<Vec<Request>> {
        let listener = listen(socket).unwrap();
        listener.set_nonblocking(false).unwrap();
        thread::spawn(move || {
            let mut heard = Vec::new();
            let (stream, _) = listener.accept().unwrap();
            answer_one(stream, &mut |request| {
                heard.push(request);
                reply.to_owned()
            })
            .unwrap();
            heard
        })
    }

    #[test]
    fn every_request_survives_the_trip_over_the_wire() {
        for request in [
            Request::Switch(Direction::Next),
            Request::Switch(Direction::Previous),
            Request::Windows,
            Request::Reload,
        ] {
            assert_eq!(request.to_string().parse(), Ok(request));
        }
    }

    #[test]
    fn a_request_reaches_the_daemon_and_its_answer_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("berth.sock");
        let daemon = daemon_answering(&socket, "ok\n");

        let answer = ask(&socket, Request::Switch(Direction::Next)).unwrap();

        assert_eq!(answer, "ok\n");
        assert_eq!(daemon.join().unwrap(), [Request::Switch(Direction::Next)]);
    }

    #[test]
    fn gibberish_is_answered_with_an_error_instead_of_being_acted_on() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("berth.sock");
        let daemon = daemon_answering(&socket, "ok\n");

        let mut stream = UnixStream::connect(&socket).unwrap();
        writeln!(stream, "open the pod bay doors").unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();

        assert_eq!(answer, "error: unknown request: open the pod bay doors\n");
        assert_eq!(daemon.join().unwrap(), []);
    }

    #[test]
    fn a_second_daemon_refuses_to_start_while_the_first_is_listening() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("berth.sock");
        let _first = listen(&socket).unwrap();

        let second = listen(&socket).unwrap_err();

        assert_eq!(second.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn only_the_user_running_the_dock_can_reach_its_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("berth.sock");
        let _listening = listen(&socket).unwrap();

        let mode = fs::metadata(&socket).unwrap().permissions().mode();

        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_socket_left_behind_by_a_crash_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("berth.sock");
        drop(listen(&socket).unwrap());

        assert!(listen(&socket).is_ok());
    }

    #[test]
    fn asking_when_no_daemon_runs_fails_instead_of_hanging() {
        let dir = tempfile::tempdir().unwrap();

        assert!(ask(&dir.path().join("berth.sock"), Request::Windows).is_err());
    }

    #[test]
    fn the_socket_is_named_after_the_wayland_display_it_serves() {
        let name = socket_path().file_name().unwrap().to_string_lossy().into_owned();

        assert!(name.starts_with("berth-") && name.ends_with(".sock"), "{name}");
    }
}
