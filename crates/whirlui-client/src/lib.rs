//! The whirl protocol client.
//!
//! A frontend is an ordinary client of the daemon's protocol: it connects to the
//! control socket, reads the greeting, reads `status`, and follows `subscribe`.
//! It owns no state of its own. The contract this crate obeys is whirl's
//! `docs/architecture.md` section 8, "Frontend contract"; the grammar it speaks
//! is section 2.
//!
//! The grammar is not re-derived here. `whirl-core` is the one implementation of
//! sections 2 and 4, and this crate depends on it, so a verb, a record form or a
//! config key cannot drift between the daemon and its frontend. What this crate
//! adds is the part `whirl-core` deliberately does not have, because it does no
//! I/O: the socket, the negotiation, the request/response path, the subscription
//! and the two recovery rules of section 8.
//!
//! What it does not do, and must never do (section 8, "must never"):
//!
//! - write a state file,
//! - call a platform setter,
//! - start, stop or restart the daemon,
//! - spawn a worker or implement a source,
//! - unlink the socket,
//! - parse the log as an interface, or
//! - poll. `subscribe` is how a client learns that something changed
//!   ([`Subscription::next`] blocks on it).
//!
//! Reading the config file to find the socket path is the one exception, and it
//! is the one the daemon's own CLI takes: a client must know where the socket is
//! before it can ask the daemon anything ([`socket_path`]). It is a path lookup,
//! not the effective plan; `status` and `sources` are the effective plan.
//!
//! ```no_run
//! use whirl_core::protocol::Request;
//! use whirlui_client::{Client, Subscription, Update};
//!
//! # fn main() -> Result<(), whirlui_client::ClientError> {
//! let mut client = Client::connect()?;
//! let status = client.status()?;
//! println!("paused: {}", status.get("paused").unwrap_or("-"));
//! client.call_ok(&Request::Next)?;
//! client.close()?;
//!
//! let mut subscription = Subscription::open()?;
//! match subscription.next() {
//!     Update::Status(status) => println!("seq {}", status.seq()),
//!     Update::Event(event) => println!("{event:?}"),
//!     Update::Offline => println!("the daemon is not running"),
//! }
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod stream;
pub mod subscription;
pub mod transport;

pub use client::{Client, ConfigCheck, Sources, Status};
pub use error::{Action, ClientError, Refusal, Socket};
pub use stream::{Event, EventStream, Signal, parse_event};
pub use subscription::{Subscription, Update};
pub use transport::{config_path, socket_path};
// The daemon's own crate, re-exported so that an app depends on this crate and
// on one pinned revision of the grammar rather than on two copies of it.
pub use whirl_core;
pub use whirl_core::protocol;

/// The protocol versions this client knows how to speak.
///
/// Section 8 item 1: a client may rely on the greeting, "read the greeting
/// before writing; send `hello` if you care; refuse to run if `protocol` is not
/// one you know". This is that list, and [`Client::connect`] refuses anything
/// outside it rather than guessing at what changed.
pub const SUPPORTED_PROTOCOLS: [u32; 1] = [whirl_core::protocol::PROTOCOL_VERSION];

/// Whether this client can speak `version`.
pub fn supports(version: u32) -> bool {
    SUPPORTED_PROTOCOLS.contains(&version)
}
