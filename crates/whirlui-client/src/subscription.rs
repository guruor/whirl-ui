//! The app-facing half of the subscription: the two recovery rules of section 8,
//! performed rather than handed to the caller.
//!
//! [`EventStream`] reports a gap, a restart and a silence; it does not act on
//! them. This is the layer that acts, and it is the layer the app uses, because
//! section 8 item 3 states the recovery as the client's obligation:
//!
//! > Compare `seq` across events and across a reconnect: a gap means you missed
//! > events and must re-read `status`; a `seq` lower than the last one you saw
//! > means the daemon restarted and you must re-read `status`.
//!
//! and item 4:
//!
//! > If nothing at all arrives for 90 s, the daemon is gone; reconnect and expect
//! > `status` to be your recovery path.
//!
//! So a caller never compares sequence numbers, never reconnects and never polls:
//! it reads [`Update`]s and draws them.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::client::{Client, Status};
use crate::error::ClientError;
use crate::stream::{Event, EventStream, LIVENESS, Signal};

/// How long to wait before trying the daemon again after it went away. A caller
/// that loops on [`Subscription::next`] therefore retries about once a second
/// rather than as fast as the CPU allows.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Something the app can draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// A complete snapshot: `status` was read, either because this subscription
    /// has just started or because a gap, a restart or a silence made the events
    /// this client was holding stale.
    Status(Status),
    /// A state change, gapless. The app can act on the event itself, and does not
    /// have to ask the daemon anything to keep its "now" line current.
    Event(Event),
    /// The daemon is not reachable right now. The caller keeps whatever it last
    /// drew; the next call retries the connection.
    Offline,
}

/// A live subscription, with the command connection it re-reads `status` on.
///
/// Two connections, which section 8 item 6 calls the intended shape: "one
/// command connection plus one subscription". The command connection is
/// [`Subscription::client`], so a caller that opens a subscription does not need
/// a third one to ask a question.
#[derive(Debug)]
pub struct Subscription {
    path: PathBuf,
    command: Client,
    stream: Option<EventStream>,
    liveness: Duration,
    reconnect_delay: Duration,
    status: Option<Status>,
    pending: VecDeque<Update>,
}

impl Subscription {
    /// Follow the daemon on the socket the three-step resolution names.
    ///
    /// The first [`Update`] is the current `status`, which is the "one `status`
    /// when the app starts" a frontend needs before it can draw anything.
    pub fn open() -> Result<Subscription, ClientError> {
        let path = crate::transport::socket_path()?;
        Subscription::open_at(&path, None)
    }

    /// Follow a named socket.
    ///
    /// `since` is section 2.9's resume point, and it decides what the first
    /// update is:
    ///
    /// - `None` starts the subscription from now, and the first update is the
    ///   current `status` snapshot.
    /// - `Some(seq)` resumes a previous subscription. The events between `seq`
    ///   and now are gone whatever happens, so a *gap* is answered with a fresh
    ///   `status` and no gap is answered with events. This is the shape the
    ///   recovery path takes: re-read the snapshot, then follow again.
    pub fn open_at(path: &Path, since: Option<u64>) -> Result<Subscription, ClientError> {
        let command = Client::connect_to(path)?;
        let stream = EventStream::open_at(path, since)?;
        let mut subscription = Subscription {
            path: path.to_path_buf(),
            command,
            stream: Some(stream),
            liveness: LIVENESS,
            reconnect_delay: RECONNECT_DELAY,
            status: None,
            pending: VecDeque::new(),
        };
        if since.is_none() {
            let status = subscription.command.status()?;
            subscription.status = Some(status.clone());
            subscription.pending.push_back(Update::Status(status));
        }
        Ok(subscription)
    }

    /// The command connection, for the verbs the app drives itself. It is the
    /// same connection `status` is re-read on, so asking it something does not
    /// cost another client slot.
    pub fn client(&mut self) -> &mut Client {
        &mut self.command
    }

    /// The last snapshot this subscription read, if it has read one.
    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    /// How long the stream waits for a line before it calls the daemon gone. The
    /// default is [`LIVENESS`] (90 s, section 8 item 4); a test lowers it rather
    /// than waiting it out.
    pub fn set_liveness(&mut self, liveness: Duration) -> Result<(), ClientError> {
        self.liveness = liveness;
        if let Some(stream) = self.stream.as_mut() {
            stream.set_liveness(liveness)?;
        }
        Ok(())
    }

    /// How long to wait before retrying after the daemon went away.
    pub fn set_reconnect_delay(&mut self, delay: Duration) {
        self.reconnect_delay = delay;
    }

    /// The next thing the app should draw.
    ///
    /// This blocks until the daemon says something, until a gap or a restart
    /// makes a re-read necessary, or until the daemon has been silent for the
    /// liveness window. It is the reason no caller of this crate polls.
    ///
    /// Not `Iterator::next`: a subscription has no end, and a caller draws the
    /// `Update` it gets rather than draining an `Option` that runs out.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Update {
        if let Some(update) = self.pending.pop_front() {
            return update;
        }
        let signal = match self.stream.as_mut() {
            Some(stream) => stream.next(),
            None => Err(ClientError::Unreachable(crate::error::Socket::unresolved())),
        };
        match signal {
            Ok(Signal::Event { event, .. }) => Update::Event(event),
            // A gap, a restart, a silence and a closed stream are the same
            // instruction: the events this client holds are no longer the state,
            // so read the state. Section 8 items 3 and 4.
            Ok(_) | Err(_) => self.recover(),
        }
    }

    /// Reconnect and re-read `status`, which is section 8's recovery path.
    fn recover(&mut self) -> Update {
        match self.reconnect() {
            Ok(status) => {
                self.status = Some(status.clone());
                Update::Status(status)
            }
            Err(_) => {
                // The daemon is gone. Drop the dead stream so the next call goes
                // straight here, wait out the retry delay, and let the caller
                // keep drawing what it has.
                self.stream = None;
                std::thread::sleep(self.reconnect_delay);
                Update::Offline
            }
        }
    }

    /// One fresh command connection and one fresh subscription.
    fn reconnect(&mut self) -> Result<Status, ClientError> {
        let mut command = Client::connect_to(&self.path)?;
        let status = command.status()?;
        let mut stream = EventStream::open_at(&self.path, None)?;
        stream.set_liveness(self.liveness)?;
        self.command = command;
        self.stream = Some(stream);
        Ok(status)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        // The daemon closes the connection either way; `close` is how a client
        // says it is finished rather than how it is allowed to end (2.3 step 4,
        // 2.9).
        if let Some(stream) = self.stream.take() {
            let _ = stream.close();
        }
    }
}
