//! The subscription: one long-lived `subscribe` connection, `seq` tracked.
//!
//! docs/architecture.md 2.9 and section 8 items 3 and 4 fix the rules this module
//! implements, and they are the reason nothing in this crate polls:
//!
//! - `seq` increases by exactly 1 per event, and `subscribed:` carries the
//!   current number so a client knows what to compare against;
//! - a gap (events were missed, or the daemon said how many) means the missed
//!   events are gone, because the daemon keeps no event history, so the client
//!   must re-read `status`;
//! - a `seq` lower than the last one seen means the daemon restarted and the
//!   counter reset, so the client must re-read `status`;
//! - nothing at all for 90 s means the daemon is gone, so the client must
//!   reconnect and re-read `status`;
//! - `seq` is never persisted and never written anywhere: a restart is detected
//!   by the number going backwards, which only works if the number is forgotten
//!   when the connection ends.
//!
//! [`EventStream`] reports those as [`Signal`]s, without acting on them.
//! [`Subscription`](crate::Subscription) is the layer that acts: it owns the
//! command connection, re-reads `status` when a signal says to, and reconnects
//! when the daemon goes away.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use whirl_core::protocol::{self, ErrorCode, LineKind, ProtocolError, Request};

use crate::client::Client;
use crate::error::{ClientError, Refusal};

/// Three missed heartbeats: docs/architecture.md 2.8's client liveness window,
/// derived from the daemon's 30 s heartbeat. Nothing at all arriving for this
/// long is the one way a client learns a subscription's daemon is gone.
pub const LIVENESS: Duration = Duration::from_secs(90);

/// One event of the closed vocabulary of docs/architecture.md 2.9.
///
/// The vocabulary is closed, but the protocol is not: section 2.4 says a new
/// event type "is not a version bump, and clients must tolerate unknown keys and
/// unknown events", so [`Event::Unknown`] keeps a type this build has never heard
/// of instead of failing the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A rotation began; `run` is the daemon's monotonic slot counter.
    RotateStart {
        run: u64,
    },
    /// The wallpaper changed; `path` takes the rest of the line.
    RotateOk {
        digest: String,
        origin_key: String,
        via: String,
        path: Option<String>,
    },
    /// The rotation produced nothing; `code` is from section 2.7.
    RotateFailed {
        code: ErrorCode,
        message: String,
    },
    /// The schedule is suspended, and `next_at` is frozen (2.5, 5.5 rule 8).
    Paused,
    /// The schedule is live again, with `next_at` re-armed from now.
    Resumed,
    FavoriteAdded {
        digest: String,
        origin_key: String,
    },
    FavoriteRemoved {
        digest: String,
    },
    /// A sweep completed; `hidden` is how many entries were kept only because
    /// they are pinned.
    CacheSwept {
        removed: u64,
        reclaimed_bytes: u64,
        hidden: u64,
    },
    /// The wall clock moved more than one interval, so the deadline was
    /// recomputed.
    ClockJump {
        seconds: i64,
    },
    /// The daemon re-read its config and it parsed.
    ConfigReloaded,
    /// The daemon does not know what is on screen and is protecting the grace
    /// window.
    AnchorUnverified,
    /// The daemon is exiting; the connection closes immediately after.
    Shutdown,
    /// 30 s of quiet. A heartbeat consumes a `seq` like any other event.
    Heartbeat {
        unix_seconds: i64,
    },
    /// An event type this build does not know (2.4).
    Unknown {
        kind: String,
        fields: Vec<String>,
    },
}

/// Parse one `event: <seq> <type> <fields...>` line.
///
/// The last field of a `rotate_ok` or a `rotate_failed` takes the rest of the
/// line, because that is what lets a path or a message contain spaces (2.2).
pub fn parse_event(line: &str) -> Option<(u64, Event)> {
    let rest = line.strip_prefix("event: ")?;
    let mut head = rest.splitn(3, ' ');
    let seq: u64 = head.next()?.parse().ok()?;
    let kind = head.next()?;
    let fields = head.next().unwrap_or("");
    let event = match kind {
        "rotate_start" => Event::RotateStart {
            run: fields.trim().parse().ok()?,
        },
        "rotate_ok" => {
            let mut parts = fields.splitn(4, ' ');
            let digest = parts.next()?.to_string();
            let origin_key = parts.next()?.to_string();
            let via = parts.next()?.to_string();
            let path = parts.next().map(|path| path.trim_end().to_string());
            Event::RotateOk {
                digest,
                origin_key,
                via,
                path: path.filter(|path| path != "-"),
            }
        }
        "rotate_failed" => {
            let (code, message) = match fields.split_once(' ') {
                Some((code, message)) => (code, message.to_string()),
                None => (fields, String::new()),
            };
            Event::RotateFailed {
                code: ErrorCode::parse(code)?,
                message,
            }
        }
        "paused" => Event::Paused,
        "resumed" => Event::Resumed,
        "favorite_added" => {
            let mut parts = fields.splitn(2, ' ');
            Event::FavoriteAdded {
                digest: parts.next()?.to_string(),
                origin_key: parts.next().unwrap_or("").to_string(),
            }
        }
        "favorite_removed" => Event::FavoriteRemoved {
            digest: fields.trim().to_string(),
        },
        "cache_swept" => {
            let mut parts = fields.split(' ');
            Event::CacheSwept {
                removed: parts.next()?.parse().ok()?,
                reclaimed_bytes: parts.next()?.parse().ok()?,
                hidden: parts.next()?.parse().ok()?,
            }
        }
        "clock_jump" => Event::ClockJump {
            seconds: fields.trim().parse().ok()?,
        },
        "config_reloaded" => Event::ConfigReloaded,
        "anchor_unverified" => Event::AnchorUnverified,
        "shutdown" => Event::Shutdown,
        "heartbeat" => Event::Heartbeat {
            unix_seconds: fields.trim().parse().ok()?,
        },
        other => Event::Unknown {
            kind: other.to_string(),
            fields: fields.split(' ').map(str::to_string).collect(),
        },
    };
    Some((seq, event))
}

/// What a subscription's stream reported, before any recovery is attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// An event, and the stream is gapless: this is exactly the next `seq`.
    Event { seq: u64, event: Event },
    /// Events were missed and are gone, so `status` must be re-read.
    Gap { missed: u64 },
    /// `seq` did not advance: the daemon restarted, so `status` must be re-read.
    Restarted { seq: u64 },
    /// Nothing at all arrived for the liveness window: the daemon is gone.
    Silence,
    /// The stream ended: the daemon shut down, or the connection was lost.
    Closed,
}

/// How an event's `seq` relates to the last one this stream saw, when the
/// relation is not "the next one".
///
/// `seq` increases by exactly 1 per event (2.9), so the two exceptions are the
/// two recoveries of section 8 item 3: a skipped value is events this client
/// never saw, and a value that did not advance is a different daemon run.
fn classify_seq(previous: Option<u64>, seq: u64) -> Option<Signal> {
    match previous {
        Some(last) if seq <= last => Some(Signal::Restarted { seq }),
        Some(last) if seq > last + 1 => Some(Signal::Gap {
            missed: seq - last - 1,
        }),
        _ => None,
    }
}

/// One `subscribe` connection, with `seq` tracked across its events.
#[derive(Debug)]
pub struct EventStream {
    client: Client,
    last_seq: Option<u64>,
    liveness: Duration,
    pending: VecDeque<Signal>,
}

impl EventStream {
    /// Subscribe on the socket the three-step resolution names, from now.
    pub fn open() -> Result<EventStream, ClientError> {
        EventStream::open_since(None)
    }

    /// Subscribe from `since`, which is the resume point of section 2.9: the
    /// daemon answers `subscribed: <current seq>` and, when `since` is older, one
    /// `gap: <n>` saying how many events are gone.
    pub fn open_since(since: Option<u64>) -> Result<EventStream, ClientError> {
        let path = crate::transport::socket_path()?;
        EventStream::open_at(&path, since)
    }

    /// Subscribe on a named socket. Used by the tests, and by
    /// [`Subscription::open_at`](crate::Subscription::open_at).
    pub fn open_at(path: &Path, since: Option<u64>) -> Result<EventStream, ClientError> {
        let client = Client::connect_to(path)?;
        let mut stream = EventStream {
            client,
            last_seq: None,
            liveness: LIVENESS,
            pending: VecDeque::new(),
        };
        // The heartbeat is the daemon's, the liveness window is the client's: a
        // read that waits longer than three missed heartbeats is the daemon being
        // gone (2.8, section 8 item 4).
        stream.client.set_read_timeout(stream.liveness)?;
        stream.client.send(&Request::Subscribe { since })?;

        let line = stream.line()?;
        if let LineKind::Err { code, message } = protocol::classify_line(&line) {
            return Err(ClientError::Refused(Refusal::new(code, message)));
        }
        let seq = line
            .strip_prefix("subscribed: ")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .ok_or_else(|| {
                ClientError::Protocol(ProtocolError::new(
                    ErrorCode::Internal,
                    format!("the daemon did not open the subscription: {line:?}"),
                ))
            })?;
        stream.last_seq = Some(seq);

        // `gap:` follows `subscribed:` exactly when `since` is older than the
        // current `seq` (2.9, and `whirld`'s own `subscribe`). Both numbers are in
        // hand, so this does not have to wait on a read to find out whether the
        // line is coming.
        match since {
            Some(since) if since < seq => {
                let line = stream.line()?;
                let missed = line
                    .strip_prefix("gap: ")
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .ok_or_else(|| {
                        ClientError::Protocol(ProtocolError::new(
                            ErrorCode::Internal,
                            format!("expected a gap line, got {line:?}"),
                        ))
                    })?;
                stream.pending.push_back(Signal::Gap { missed });
            }
            // The resume point is ahead of the daemon: `seq` starts at 1 per run
            // and is never persisted, so a lower number is a different daemon
            // (2.9, section 8 item 3).
            Some(since) if since > seq => {
                stream.pending.push_back(Signal::Restarted { seq });
            }
            _ => {}
        }
        Ok(stream)
    }

    /// How long this stream waits for a line before calling the daemon gone.
    /// The default is [`LIVENESS`]; a test that must not wait 90 s for a
    /// negative result lowers it.
    pub fn set_liveness(&mut self, liveness: Duration) -> Result<(), ClientError> {
        self.liveness = liveness;
        self.client.set_read_timeout(liveness)
    }

    /// The last `seq` this stream saw, from `subscribed:` or from an event.
    pub fn last_seq(&self) -> Option<u64> {
        self.last_seq
    }

    /// The next thing the stream has to say.
    ///
    /// `Err` is a connection that failed rather than a daemon that spoke; a
    /// caller recovers from both the same way, by re-reading `status`.
    ///
    /// Not `Iterator::next`: a subscription has no end, and what it hands back
    /// is a state to draw rather than an `Option` that runs out.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Signal, ClientError> {
        if let Some(signal) = self.pending.pop_front() {
            return Ok(signal);
        }
        loop {
            let line = match self.client.read_line() {
                Ok(Some(line)) => line,
                Ok(None) => return Ok(Signal::Closed),
                Err(error) if error.timed_out() => return Ok(Signal::Silence),
                Err(error) => return Err(error),
            };
            match protocol::classify_line(&line) {
                LineKind::Event => {
                    let (seq, event) = parse_event(&line).ok_or_else(|| {
                        ClientError::Protocol(ProtocolError::new(
                            ErrorCode::Internal,
                            format!("an event line the grammar does not allow: {line:?}"),
                        ))
                    })?;
                    let previous = self.last_seq.replace(seq);
                    return Ok(classify_seq(previous, seq).unwrap_or(Signal::Event { seq, event }));
                }
                // The daemon answers anything a client sends during the stream
                // with `ERR bad_args subscribe takes over this connection` and
                // keeps the stream going (2.9). This client sends nothing, and
                // treats a refusal as the stream being unusable rather than
                // silently skipping it.
                LineKind::Err { code, message } => {
                    return Err(ClientError::Refused(Refusal::new(code, message)));
                }
                _ => continue,
            }
        }
    }

    /// End the subscription: `close` is the one request a subscribed connection
    /// still honours (2.3 step 5, 2.9).
    pub fn close(mut self) -> Result<(), ClientError> {
        self.client.send(&Request::Close)?;
        loop {
            match self.client.read_line() {
                Ok(Some(line)) => match protocol::classify_line(&line) {
                    LineKind::Ok => return Ok(()),
                    LineKind::Err { code, message } => {
                        return Err(ClientError::Refused(Refusal::new(code, message)));
                    }
                    _ => continue,
                },
                Ok(None) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }

    /// One line, or the error the read produced.
    fn line(&mut self) -> Result<String, ClientError> {
        self.client.read_line()?.ok_or_else(|| {
            ClientError::Protocol(ProtocolError::new(
                ErrorCode::Internal,
                "the daemon closed the subscription before it opened",
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_of_the_closed_vocabulary_parses() {
        let cases = [
            (
                "event: 184 rotate_start 4211",
                Event::RotateStart { run: 4211 },
            ),
            (
                "event: 185 rotate_ok 26b8 pictures:1cc4 source /a path/with spaces.jpg",
                Event::RotateOk {
                    digest: "26b8".to_string(),
                    origin_key: "pictures:1cc4".to_string(),
                    via: "source".to_string(),
                    path: Some("/a path/with spaces.jpg".to_string()),
                },
            ),
            (
                "event: 195 rotate_failed set_failed the platform refused the set",
                Event::RotateFailed {
                    code: ErrorCode::SetFailed,
                    message: "the platform refused the set".to_string(),
                },
            ),
            ("event: 186 paused", Event::Paused),
            ("event: 187 resumed", Event::Resumed),
            (
                "event: 190 favorite_added ab12cd space:ab12cd",
                Event::FavoriteAdded {
                    digest: "ab12cd".to_string(),
                    origin_key: "space:ab12cd".to_string(),
                },
            ),
            (
                "event: 191 favorite_removed ab12cd",
                Event::FavoriteRemoved {
                    digest: "ab12cd".to_string(),
                },
            ),
            (
                "event: 189 cache_swept 12 34816000 1",
                Event::CacheSwept {
                    removed: 12,
                    reclaimed_bytes: 34_816_000,
                    hidden: 1,
                },
            ),
            (
                "event: 190 clock_jump 7200",
                Event::ClockJump { seconds: 7200 },
            ),
            ("event: 188 config_reloaded", Event::ConfigReloaded),
            ("event: 191 anchor_unverified", Event::AnchorUnverified),
            ("event: 192 shutdown", Event::Shutdown),
            (
                "event: 192 heartbeat 1790324533",
                Event::Heartbeat {
                    unix_seconds: 1_790_324_533,
                },
            ),
        ];
        for (line, expected) in cases {
            let (_, event) = parse_event(line).unwrap_or_else(|| panic!("{line}"));
            assert_eq!(event, expected, "{line}");
        }
        // A type this build does not know is kept, never a failure (2.4).
        let (seq, event) = parse_event("event: 200 something_new a b").expect("an unknown event");
        assert_eq!(seq, 200);
        assert_eq!(
            event,
            Event::Unknown {
                kind: "something_new".to_string(),
                fields: vec!["a".to_string(), "b".to_string()],
            }
        );
        assert_eq!(parse_event("event: not-a-seq paused"), None);
        assert_eq!(parse_event("paused"), None);
    }

    #[test]
    fn a_rotate_ok_with_no_path_is_a_reference_set() {
        let (_, event) =
            parse_event("event: 2 rotate_ok ab pictures:ab manual -").expect("an event");
        assert_eq!(
            event,
            Event::RotateOk {
                digest: "ab".to_string(),
                origin_key: "pictures:ab".to_string(),
                via: "manual".to_string(),
                path: None,
            }
        );
    }

    #[test]
    fn a_seq_that_is_not_the_next_one_is_a_gap_or_a_restart() {
        // The next one: no signal, just the event.
        assert_eq!(classify_seq(Some(4), 5), None);
        assert_eq!(classify_seq(None, 1), None);
        // A skipped value is events this client never saw.
        assert_eq!(classify_seq(Some(4), 6), Some(Signal::Gap { missed: 1 }));
        assert_eq!(classify_seq(Some(4), 9), Some(Signal::Gap { missed: 4 }));
        // A value that did not advance is a different daemon run.
        assert_eq!(classify_seq(Some(4), 4), Some(Signal::Restarted { seq: 4 }));
        assert_eq!(classify_seq(Some(4), 1), Some(Signal::Restarted { seq: 1 }));
    }
}
