//! The command connection: negotiation, then one request and one response at a
//! time.
//!
//! The daemon answers each request before it reads the next, so responses arrive
//! in request order and one connection can serve every verb the app has
//! (docs/architecture.md 2.3). A subscription is *not* one of them: `subscribe`
//! takes the connection over, so [`EventStream`](crate::EventStream) opens its
//! own (2.3 step 5), and section 8 item 6 calls that shape the intended one.
//!
//! The greeting is read before anything is written, and a greeting this client
//! cannot speak to is a refusal to run rather than a warning: docs/architecture.md
//! 2.4 makes the version number "a claim about parseability, not about feature
//! sets", so a client that guessed at a version it does not know would be
//! guessing at the grammar.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Duration;

use whirl_core::protocol::{
    self, ErrorCode, Greeting, LineKind, ProtocolError, Request, Response, SourceRecord,
    parse_source_record,
};

use crate::error::{ClientError, Refusal};
use crate::transport::{self, Stream};

/// The client's own name on the wire: `<name>/<version>`, which is what section
/// 2.4's optional `hello` argument carries.
pub fn client_name() -> String {
    format!("whirlui-client/{}", env!("CARGO_PKG_VERSION"))
}

/// How long a client waits for a response before giving up.
///
/// docs/architecture.md 2.8: the longest a verb can take is the rotation request
/// timeout, 300 s, "deliberately equal to the worker deadline: a client must not
/// give up before the daemon does, or it will report a failure for a rotation
/// that then succeeds". This is that number, and the daemon's own CLI uses it.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// One connection to the daemon, negotiated and ready for requests.
#[derive(Debug)]
pub struct Client {
    writer: Stream,
    reader: BufReader<Stream>,
    greeting: Greeting,
}

impl Client {
    /// Connect to the socket the three-step resolution names
    /// ([`socket_path`](crate::socket_path)).
    pub fn connect() -> Result<Client, ClientError> {
        let path = transport::socket_path()?;
        Client::connect_to(&path)
    }

    /// Connect to a named socket, negotiate, and return a usable client.
    pub fn connect_to(path: &Path) -> Result<Client, ClientError> {
        let stream = transport::connect(path)?;
        stream
            .set_read_timeout(Some(COMMAND_TIMEOUT))
            .map_err(ClientError::Io)?;
        // Reading happens on a clone so that the greeting can be read while the
        // original stays available for writing, the way the daemon's own test
        // harness does it. A read timeout is a property of the socket, so setting
        // it on the original covers the clone.
        let reader = BufReader::new(stream.try_clone().map_err(ClientError::Io)?);
        let mut client = Client {
            writer: stream,
            reader,
            greeting: Greeting {
                product: protocol::PRODUCT.to_string(),
                version: protocol::VERSION.to_string(),
                protocol: protocol::PROTOCOL_VERSION,
            },
        };

        // 1. The greeting, before anything is written (2.3 step 1).
        let line = client.read_line()?.ok_or_else(|| {
            ClientError::Protocol(ProtocolError::new(
                ErrorCode::Internal,
                "the daemon closed the connection without a greeting",
            ))
        })?;
        let greeting = protocol::parse_greeting(&line)
            .ok_or_else(|| ClientError::MalformedGreeting { line: line.clone() })?;

        // 2. Refuse a version this client does not know (2.4, section 8 item 1).
        // This is the check `docs/milestones.md` M1 criterion 2 names: it is a
        // behaviour, and `refuses_an_unknown_protocol_version` fails when it is
        // removed.
        if !crate::supports(greeting.protocol) {
            return Err(ClientError::UnknownProtocol {
                server: greeting.protocol.to_string(),
                supported: &crate::SUPPORTED_PROTOCOLS,
            });
        }
        client.greeting = greeting;

        // 3. `hello`, which is the version announcement a client that cares sends
        // (2.4). The reply is `protocol: <m>` and `OK`; anything else is the
        // daemon refusing to talk to this client, which is already an `ERR`.
        let lines = client.call_ok(&Request::Hello {
            version: protocol::PROTOCOL_VERSION,
            client: Some(client_name()),
        })?;
        if let Some(agreed) = protocol_line(&lines) {
            if agreed != protocol::PROTOCOL_VERSION {
                return Err(ClientError::Protocol(ProtocolError::new(
                    ErrorCode::BadProtocol,
                    format!(
                        "the daemon negotiated protocol {agreed}, which this client cannot speak"
                    ),
                )));
            }
        }
        Ok(client)
    }

    /// The greeting the daemon sent.
    pub fn greeting(&self) -> &Greeting {
        &self.greeting
    }

    /// How long a read may take before it fails. The default is
    /// [`COMMAND_TIMEOUT`]; a caller that drives a test daemon may want less.
    pub fn set_read_timeout(&self, timeout: Duration) -> Result<(), ClientError> {
        self.writer
            .set_read_timeout(Some(timeout))
            .map_err(ClientError::Io)
    }

    /// Send one request and read its whole response.
    ///
    /// A response is every line up to and including its terminator; a terminator
    /// of `ERR` is a legal answer and comes back as `Ok`, so the caller decides
    /// what a refusal means by its code (2.7). Only a framing failure, a closed
    /// connection or an I/O error is an `Err`.
    pub fn call(&mut self, request: &Request) -> Result<Response, ClientError> {
        self.send(request)?;
        self.read_response()
    }

    /// Write one request line and flush it, without waiting for an answer.
    ///
    /// This is what `subscribe` needs: it takes the connection over, so its
    /// answer is a stream rather than a response ([`EventStream`](crate::EventStream)).
    pub(crate) fn send(&mut self, request: &Request) -> Result<(), ClientError> {
        let line = request.encode();
        if line.len() > protocol::MAX_REQUEST_LINE {
            return Err(ClientError::Protocol(ProtocolError::new(
                ErrorCode::TooLong,
                format!("request line exceeds {} bytes", protocol::MAX_REQUEST_LINE),
            )));
        }
        writeln!(self.writer, "{line}").map_err(ClientError::Io)?;
        self.writer.flush().map_err(ClientError::Io)
    }

    /// [`call`](Client::call), with an `ERR` terminator turned into
    /// [`ClientError::Refused`].
    ///
    /// The data lines, in order, on success. This is the path every verb of
    /// milestone 1 takes.
    pub fn call_ok(&mut self, request: &Request) -> Result<Vec<String>, ClientError> {
        let response = self.call(request)?;
        lines_of(response)
    }

    /// `status`: the complete snapshot a frontend draws from (2.10).
    pub fn status(&mut self) -> Result<Status, ClientError> {
        let lines = self.call_ok(&Request::Status)?;
        Ok(Status::from_lines(&lines))
    }

    /// `sources`: the enabled sources and the reason each refused one is off
    /// (2.6, 2.10).
    pub fn sources(&mut self) -> Result<Sources, ClientError> {
        let lines = self.call_ok(&Request::Sources)?;
        Ok(Sources::from_lines(&lines))
    }

    /// `config path`: the config file the daemon read.
    pub fn config_path(&mut self) -> Result<String, ClientError> {
        Ok(value_of(self.call_ok(&Request::ConfigPath)?, "config"))
    }

    /// `config check`: the effective plan, as the daemon adopted it (2.6).
    pub fn config_check(&mut self) -> Result<ConfigCheck, ClientError> {
        let lines = self.call_ok(&Request::ConfigCheck)?;
        Ok(ConfigCheck::from_lines(&lines))
    }

    /// `version`: the daemon's version, the protocol version and the platform.
    pub fn version(&mut self) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::Version)
    }

    /// `ping`: one round trip that does no work (2.5.1).
    pub fn ping(&mut self) -> Result<(), ClientError> {
        self.call_ok(&Request::Ping).map(|_| ())
    }

    /// `next`: rotate now. Blocks until the daemon has an outcome (up to 300 s).
    ///
    /// The name is the daemon's verb, not `Iterator::next`: section 2.5 calls
    /// this `next`, and a different name here would be a lie about the
    /// protocol.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::Next)
    }

    /// `prev`: step back one entry in the history ring, mutating nothing (2.5).
    pub fn prev(&mut self) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::Prev)
    }

    /// `set path`: set this image, by absolute path.
    pub fn set_path(&mut self, path: &str) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::SetPath(path.to_string()))
    }

    /// `set id`: set this image, by `origin_key` or digest.
    pub fn set_id(&mut self, id: &str) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::SetId(id.to_string()))
    }

    /// `pause`: freeze the schedule.
    pub fn pause(&mut self) -> Result<(), ClientError> {
        self.call_ok(&Request::Pause).map(|_| ())
    }

    /// `resume`: re-arm the schedule from now.
    pub fn resume(&mut self) -> Result<(), ClientError> {
        self.call_ok(&Request::Resume).map(|_| ())
    }

    /// `favorite [<id>]`: pin the current entry, or the one named.
    pub fn favorite(&mut self, id: Option<&str>) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::Favorite {
            id: id.map(str::to_string),
        })
    }

    /// `unfavorite <id>`: unpin.
    pub fn unfavorite(&mut self, id: &str) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::Unfavorite(id.to_string()))
    }

    /// `favorites`: the pinned entries.
    pub fn favorites(&mut self) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::Favorites)
    }

    /// `history [<n>]`: the last `count` entries, newest first.
    pub fn history(&mut self, count: usize) -> Result<Vec<String>, ClientError> {
        self.call_ok(&Request::History { count })
    }

    /// `close`: end this connection cleanly. The daemon answers `OK` and then
    /// closes (2.3 step 4).
    pub fn close(mut self) -> Result<(), ClientError> {
        self.call_ok(&Request::Close).map(|_| ())
    }

    /// Every line up to and including the terminator, as a [`Response`].
    fn read_response(&mut self) -> Result<Response, ClientError> {
        let mut lines: Vec<String> = Vec::new();
        loop {
            let line = self.read_line()?.ok_or_else(|| {
                ClientError::Protocol(ProtocolError::new(
                    ErrorCode::Internal,
                    "the daemon closed the connection without a terminator",
                ))
            })?;
            match protocol::classify_line(&line) {
                LineKind::Ok => {
                    let mut response = Response::ok();
                    for line in lines {
                        response = response.line(line);
                    }
                    return Ok(response);
                }
                LineKind::Err { code, message } => {
                    let mut response = Response::err(code, message);
                    for line in lines {
                        response = response.line(line);
                    }
                    return Ok(response);
                }
                LineKind::Malformed => {
                    return Err(ClientError::Protocol(ProtocolError::new(
                        ErrorCode::Internal,
                        format!("a line the grammar does not allow: {line:?}"),
                    )));
                }
                _ => lines.push(line),
            }
        }
    }

    /// One line, with its terminator removed. `None` at end of stream.
    pub(crate) fn read_line(&mut self) -> Result<Option<String>, ClientError> {
        let mut buffer = String::new();
        let read = self
            .reader
            .read_line(&mut buffer)
            .map_err(ClientError::Io)?;
        if read == 0 {
            return Ok(None);
        }
        Ok(Some(buffer.trim_end_matches(['\n', '\r']).to_string()))
    }
}

/// `lines_of`, but an `ERR` terminator becomes a typed refusal.
fn lines_of(response: Response) -> Result<Vec<String>, ClientError> {
    match response.terminator() {
        protocol::Terminator::Ok => Ok(response.lines().to_vec()),
        protocol::Terminator::Err { code, message } => {
            Err(ClientError::Refused(Refusal::new(*code, message.clone())))
        }
    }
}

/// The value of the first `key: value` line, or `-` when the key is absent.
fn value_of(lines: Vec<String>, key: &str) -> String {
    let prefix = format!("{key}: ");
    lines
        .into_iter()
        .find_map(|line| line.strip_prefix(&prefix).map(str::to_string))
        .unwrap_or_else(|| "-".to_string())
}

/// The `protocol: <n>` value of a `hello` reply, when it carries one.
fn protocol_line(lines: &[String]) -> Option<u32> {
    let prefix = "protocol: ";
    let value = lines.iter().find_map(|line| line.strip_prefix(prefix))?;
    value.trim().parse().ok()
}

/// `status`, as the daemon prints it (2.10): an ordered set of `key: value`
/// lines plus one `source:` record per source.
///
/// The key set is a contract and it grows, so the unknown keys of a newer daemon
/// are kept rather than refused (2.4: "clients must tolerate unknown keys"), and
/// a caller reads the ones it draws from by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    keys: Vec<(String, String)>,
    sources: Vec<SourceRecord>,
}

impl Status {
    /// Parse a `status` response body.
    pub fn from_lines(lines: &[String]) -> Status {
        let mut keys = Vec::new();
        let mut sources = Vec::new();
        for line in lines {
            if let Some(record) = parse_source_record(line) {
                sources.push(record);
                continue;
            }
            if let Some((key, value)) = line.split_once(": ") {
                keys.push((key.to_string(), value.to_string()));
            }
        }
        Status { keys, sources }
    }

    /// The value of `key`, if the daemon sent it. `None` is "not this version";
    /// `Some("-")` is "unset", which is a different fact (2.6).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Whether `key` is the flag `1`.
    pub fn flag(&self, key: &str) -> bool {
        self.get(key) == Some("1")
    }

    /// The value of `key` as a number, when it is one.
    pub fn number(&self, key: &str) -> Option<u64> {
        self.get(key)?.parse().ok()
    }

    /// Every `key: value` line, in the daemon's own order.
    pub fn lines(&self) -> &[(String, String)] {
        &self.keys
    }

    /// The `source:` records, in config order.
    pub fn source_records(&self) -> &[SourceRecord] {
        &self.sources
    }

    /// The protocol version the daemon reported.
    pub fn protocol(&self) -> Option<u32> {
        self.number("protocol")
            .and_then(|value| value.try_into().ok())
    }

    /// The state sequence number: every state change increments it (2.9).
    pub fn seq(&self) -> u64 {
        self.number("seq").unwrap_or(0)
    }

    /// Whether the schedule is suspended.
    pub fn paused(&self) -> bool {
        self.flag("paused")
    }

    /// Whether a worker is running.
    pub fn rotating(&self) -> bool {
        self.flag("rotating")
    }

    /// Whether pin state is unknown, so the pin affordance should be hidden
    /// (section 8 item 5).
    pub fn favorites_degraded(&self) -> bool {
        self.flag("favorites_degraded")
    }

    /// How many of the configured sources are enabled.
    pub fn sources_enabled(&self) -> usize {
        self.number("sources").unwrap_or(0) as usize
    }
}

/// A `sources` response: the count the daemon printed and its records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sources {
    pub count: usize,
    pub records: Vec<SourceRecord>,
}

impl Sources {
    pub fn from_lines(lines: &[String]) -> Sources {
        let mut records = Vec::new();
        let mut count = None;
        for line in lines {
            if let Some(record) = parse_source_record(line) {
                records.push(record);
                continue;
            }
            if count.is_none() {
                count = line
                    .strip_prefix("count: ")
                    .and_then(|value| value.trim().parse().ok());
            }
        }
        Sources {
            count: count.unwrap_or(records.len()),
            records,
        }
    }
}

/// A `config check` response: the per-source plan and the one `plan:` line
/// (2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigCheck {
    pub records: Vec<SourceRecord>,
    pub plan: Vec<(String, String)>,
    /// Every line the daemon sent, in order, so a caller can print what the
    /// daemon actually said rather than a re-rendering of it.
    pub lines: Vec<String>,
}

impl ConfigCheck {
    pub fn from_lines(lines: &[String]) -> ConfigCheck {
        let mut records = Vec::new();
        let mut plan = Vec::new();
        for line in lines {
            if let Some(record) = parse_source_record(line) {
                records.push(record);
                continue;
            }
            if let Some(pairs) = protocol::parse_plan_record(line) {
                plan.extend(pairs);
            }
        }
        ConfigCheck {
            records,
            plan,
            lines: lines.to_vec(),
        }
    }

    /// The effective value of one `plan:` key path.
    pub fn effective(&self, key: &str) -> Option<&str> {
        self.plan
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_keeps_its_keys_and_parses_its_sources() {
        let lines: Vec<String> = [
            "daemon_version: whirl 0.1.0",
            "protocol: 2",
            "seq: 183",
            "paused: 1",
            "rotating: 0",
            "next_in_s: -",
            "favorites_degraded: 0",
            "sources: 2",
            "source: pictures local weight=1 enabled=1 last=ok reason=-",
            "source: archive local weight=3 enabled=1 last=- reason=-",
            "a_key_this_build_does_not_know: whatever",
        ]
        .iter()
        .map(|line| line.to_string())
        .collect();
        let status = Status::from_lines(&lines);
        assert_eq!(status.protocol(), Some(2));
        assert_eq!(status.seq(), 183);
        assert!(status.paused());
        assert!(!status.rotating());
        assert!(!status.favorites_degraded());
        assert_eq!(status.sources_enabled(), 2);
        assert_eq!(status.get("next_in_s"), Some("-"));
        assert_eq!(status.get("no_such_key"), None);
        // An unknown key is kept, never an error (2.4).
        assert_eq!(
            status.get("a_key_this_build_does_not_know"),
            Some("whatever")
        );
        assert_eq!(status.source_records().len(), 2);
        assert_eq!(status.source_records()[0].id, "pictures");
    }

    #[test]
    fn a_config_check_keeps_the_records_and_the_plan() {
        let lines: Vec<String> = [
            "queued",
            "source: pictures local weight=1 enabled=1 last=- candidates=412 admitted=97 reason=-",
            "plan: schedule.interval_seconds=1800 backend=native sources=2",
        ]
        .iter()
        .map(|line| line.to_string())
        .collect();
        let check = ConfigCheck::from_lines(&lines);
        assert_eq!(check.records.len(), 1);
        assert_eq!(check.effective("schedule.interval_seconds"), Some("1800"));
        assert_eq!(check.effective("backend"), Some("native"));
        assert_eq!(check.lines.len(), 3);
    }

    #[test]
    fn a_result_line_is_read_by_key_or_reported_as_unset() {
        let lines = vec!["config: /where/ever/config.json".to_string()];
        assert_eq!(value_of(lines, "config"), "/where/ever/config.json");
        assert_eq!(value_of(vec!["OK".to_string()], "config"), "-");
    }
}
