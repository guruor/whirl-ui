//! Test support: a scratch tree, a real daemon when one can be found, and a stub
//! daemon for the failure paths a real daemon will not produce on demand.
//!
//! Everything here is unix-only, because the client's transport is.
//!
//! The module is compiled into each integration test binary, and no one binary
//! uses all of it, so the unused-item warnings below are noise rather than
//! signal. `cargo clippy --all-targets -D warnings` would otherwise fail on a
//! module that is merely shared.
#![cfg(unix)]
#![allow(dead_code)]

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Makes every scratch directory unique within a test process.
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A directory that removes itself.
#[derive(Debug)]
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// A fresh directory, named for the test that asked for it.
    pub fn new(tag: &str) -> Scratch {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = env::temp_dir().join(format!("whirlui-{tag}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A path inside it.
    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The daemon binary, from wherever this machine keeps it.
///
/// A test that needs a real daemon does not fail without one: `cargo test
/// --workspace` runs in CI with no daemon built, and a frontend's test suite is
/// not the place to build the daemon from source. Point `WHIRL_DAEMON` at a
/// `whirld` to run those tests; the rest of the suite still covers the grammar,
/// the negotiation and the recovery rules.
pub fn daemon_binary() -> Option<PathBuf> {
    if let Some(path) = env::var_os("WHIRL_DAEMON") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    // Beside this workspace's own build output, which is where a daemon built
    // from the pinned revision lands.
    if let Ok(executable) = env::current_exe() {
        // <target>/<profile>/deps/<test>, so the target root is three up.
        if let Some(root) = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
        {
            for candidate in ["whirl-daemon/debug/whirld", "debug/whirld"] {
                let path = root.join(candidate);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    // On PATH, the way an installed daemon is found.
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join("whirld"))
        .find(|candidate| candidate.is_file())
}

/// A real `whirld` on a scratch tree, with a config that has a source in it.
///
/// This is a test harness, not a frontend behaviour: section 8 forbids a
/// frontend from starting, stopping or restarting a daemon, and the only reason
/// a daemon appears here is that a test has to have something to talk to. Every
/// daemon this file starts is on a scratch tree, with `WHIRL_BACKEND=noop`, and
/// is killed by the `Drop` below.
#[derive(Debug)]
pub struct Daemon {
    scratch: Scratch,
    child: Child,
    /// The first source's directory, where `plant` puts images.
    walls: PathBuf,
}

impl Daemon {
    /// Start a daemon with one local source pointing at an empty directory.
    pub fn start() -> Daemon {
        Daemon::start_with(&[("pictures", "local", 1)])
    }

    /// Start a daemon whose config has these sources, as `(id, kind, weight)`.
    pub fn start_with(sources: &[(&str, &str, u32)]) -> Daemon {
        let binary = daemon_binary().expect("a whirld binary");
        let scratch = Scratch::new("daemon");
        std::fs::create_dir_all(scratch.join("state")).expect("a state directory");
        std::fs::create_dir_all(scratch.join("cache")).expect("a cache directory");

        let mut listed = Vec::new();
        for (id, kind, weight) in sources {
            let wallpapers = scratch.join(id);
            std::fs::create_dir_all(&wallpapers).expect("a source directory");
            listed.push(format!(
                "{{ \"id\": \"{id}\", \"kind\": \"{kind}\", \"weight\": {weight}, \
                 \"paths\": [\"{}\"] }}",
                wallpapers.display()
            ));
        }
        let walls = sources.first().map_or_else(
            || scratch.path().to_path_buf(),
            |(id, _, _)| scratch.join(id),
        );
        let config = scratch.join("config.json");
        std::fs::write(
            &config,
            format!(
                "{{ \"config_schema\": 1, \"sources\": [{}] }}",
                listed.join(", ")
            ),
        )
        .expect("a config file");

        let child = Command::new(binary)
            .env("WHIRL_SOCKET", scratch.join("w.sock"))
            .env("WHIRL_CONFIG", &config)
            .env("WHIRL_STATE_DIR", scratch.join("state"))
            .env("WHIRL_CACHE_DIR", scratch.join("cache"))
            // No platform setter: these tests are about the protocol, and a test
            // that changed the wallpaper would change the machine it runs on.
            .env("WHIRL_BACKEND", "noop")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a running whirld");

        let daemon = Daemon {
            scratch,
            child,
            walls,
        };
        daemon.wait_for_socket();
        daemon
    }

    pub fn socket(&self) -> PathBuf {
        self.scratch.join("w.sock")
    }

    pub fn dir(&self) -> &Path {
        self.scratch.path()
    }

    /// The config file this daemon read.
    pub fn config(&self) -> PathBuf {
        self.scratch.join("config.json")
    }

    /// An image in the first source's directory, and its path.
    ///
    /// A PNG header: the signature, the IHDR chunk's length and type, and the
    /// dimensions, then the five bytes that follow them and a zero CRC placeholder.
    /// The dimensions are inside the first 24 bytes, which is the whole of the
    /// header read whirl's own worker makes (it validates a header and never
    /// decodes a body), and the same fixture its tests plant.
    pub fn plant(&self, name: &str, width: u32, height: u32) -> PathBuf {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let path = self.walls.join(name);
        std::fs::write(&path, bytes).expect("a planted image");
        path
    }

    /// This daemon's pid, for the check that nothing outlives the test.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the process is still running, without waiting on it.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Wait for the socket to exist, and fail loudly rather than flakily if the
    /// daemon never gets there.
    ///
    /// The message names the file, not the directory it lives in: a diagnostic
    /// that a failing test prints is still a diagnostic, and the constraint that
    /// nothing this crate prints carries a home path applies to it too.
    fn wait_for_socket(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.socket().exists() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("whirld did not create its socket file within 20 s");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What a stub daemon answers with.
#[derive(Debug, Clone)]
pub enum Reply {
    /// Nothing, and keep the connection open.
    Silence,
    /// These lines, in order, and keep the connection open.
    Lines(Vec<String>),
    /// These lines, then close the connection.
    LinesThenClose(Vec<String>),
}

/// A scripted daemon on a scratch socket.
///
/// The failure paths of section 8 are mostly things a healthy daemon does not do
/// on demand: announce a protocol version this client cannot speak, open a
/// subscription and then say nothing. A stub produces them deterministically, on
/// every platform, including the ones with no daemon installed.
pub struct Stub {
    scratch: Scratch,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    received: Arc<Mutex<String>>,
    acceptor: Option<JoinHandle<()>>,
}

impl Stub {
    /// Listen on a scratch socket and answer each request line with `reply`.
    pub fn start<F>(greeting: &str, reply: F) -> Stub
    where
        F: Fn(&str, &mut UnixStream) -> Reply + Send + Sync + 'static,
    {
        let scratch = Scratch::new("stub");
        let path = scratch.join("w.sock");
        let listener = UnixListener::bind(&path).expect("a stub socket");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking stub socket");

        let stop = Arc::new(AtomicBool::new(false));
        let received = Arc::new(Mutex::new(String::new()));
        let greeting = greeting.to_string();
        let reply = Arc::new(reply);
        let acceptor = {
            let stop = Arc::clone(&stop);
            let received = Arc::clone(&received);
            thread::spawn(move || {
                let mut connections: Vec<(UnixStream, JoinHandle<()>)> = Vec::new();
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            // macOS and the other BSDs hand back an accepted
                            // socket that inherited O_NONBLOCK from this
                            // non-blocking listener, so the first read returns
                            // WouldBlock at once. A real daemon's accepted socket
                            // blocks, and this one has to as well: `serve` reads
                            // the socket it was given, and a socket that never
                            // blocks would look like a peer that had already gone.
                            let _ = stream.set_nonblocking(false);
                            let control = stream.try_clone().expect("a control handle");
                            let greeting = greeting.clone();
                            let reply = Arc::clone(&reply);
                            let received = Arc::clone(&received);
                            connections.push((
                                control,
                                thread::spawn(move || {
                                    serve(stream, &greeting, reply.as_ref(), &received)
                                }),
                            ));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
                // Shutting the socket down is what releases a connection thread
                // blocked on a read, so nothing has to poll a timeout.
                for (control, handle) in connections {
                    let _ = control.shutdown(Shutdown::Both);
                    let _ = handle.join();
                }
            })
        };

        // `bind` creates the file before it returns, so a client that connects
        // immediately will find it.
        Stub {
            scratch,
            path,
            stop,
            received,
            acceptor: Some(acceptor),
        }
    }

    pub fn socket(&self) -> &Path {
        &self.path
    }

    /// Everything a client has written to this stub so far.
    pub fn received(&self) -> String {
        self.received.lock().expect("the received log").clone()
    }

    /// Stop listening, close the connections and remove the socket.
    pub fn stop(&mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = &self.scratch;
        self.shut_down();
    }
}

/// One connection: greeting first, then a reply per request line.
fn serve(
    stream: UnixStream,
    greeting: &str,
    reply: &(dyn Fn(&str, &mut UnixStream) -> Reply + Send + Sync),
    received: &Mutex<String>,
) {
    let mut writer = match stream.try_clone() {
        Ok(writer) => writer,
        Err(_) => return,
    };
    if writeln!(writer, "{greeting}").is_err() || writer.flush().is_err() {
        return;
    }
    let mut reader = BufReader::new(stream);
    loop {
        let mut buffer = String::new();
        match reader.read_line(&mut buffer) {
            Ok(0) => return,
            Ok(_) => {
                let line = buffer.trim_end_matches(['\n', '\r']).to_string();
                received
                    .lock()
                    .expect("the received log")
                    .push_str(&format!("{line}\n"));
                match reply(&line, &mut writer) {
                    Reply::Silence => {}
                    Reply::Lines(lines) => {
                        if write_lines(&mut writer, &lines).is_err() {
                            return;
                        }
                    }
                    Reply::LinesThenClose(lines) => {
                        let _ = write_lines(&mut writer, &lines);
                        return;
                    }
                }
            }
            Err(_) => return,
        }
    }
}

fn write_lines(writer: &mut UnixStream, lines: &[String]) -> std::io::Result<()> {
    for line in lines {
        writeln!(writer, "{line}")?;
    }
    writer.flush()
}

/// A `status` response body, as the harness's stubs and assertions both use it.
pub fn stub_status_lines(paused: bool, seq: u64) -> Vec<String> {
    vec![
        "daemon_version: whirl 0.1.0".to_string(),
        "protocol: 2".to_string(),
        format!("seq: {seq}"),
        format!("paused: {}", u8::from(paused)),
        "rotating: 0".to_string(),
        "next_in_s: -".to_string(),
        "favorites_degraded: 0".to_string(),
        "sources: 1".to_string(),
        "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
    ]
}

/// Answer a stub's `hello` with the negotiation a healthy daemon gives.
pub fn hello_reply() -> Vec<String> {
    vec!["protocol: 2".to_string(), "OK".to_string()]
}

/// `OK` on its own, the answer to everything that changes nothing.
pub fn ok_reply() -> Vec<String> {
    vec!["OK".to_string()]
}

/// A stub's data lines with the terminator a real response ends with.
pub fn ok_after(mut lines: Vec<String>) -> Vec<String> {
    lines.extend(ok_reply());
    lines
}
