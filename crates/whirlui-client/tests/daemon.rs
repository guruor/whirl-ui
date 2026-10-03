//! The calls, against a real `whirld`.
//!
//! `negotiation.rs` covers what a frontend does when the far side is not a
//! healthy daemon, with a scripted stub for the states a real daemon will not
//! produce on demand. This file is the other half: one real daemon, started on a
//! scratch tree by the test itself, answering the verbs milestone 1 needs. It is
//! the shape `crates/whirld/tests/control_socket.rs` uses in whirl's own tree.
//!
//! Unix only, because the transport of docs/architecture.md 2.1 is a Unix domain
//! socket in this build. A machine with no `whirld` to point at skips these
//! tests loudly rather than failing: `cargo test --workspace` in CI has no
//! daemon built, and building one is not a frontend test's job. Set
//! `WHIRL_DAEMON` (or build one into `target/whirl-daemon/debug/`) to run them.
//!
//! The daemon here is a **test harness, not a frontend behaviour**: section 8's
//! "must never" list forbids a frontend from starting, stopping or restarting a
//! daemon, and nothing outside this tests directory does any of it.
#![cfg(unix)]

mod support;

use std::path::{Path, PathBuf};

use support::{Daemon, daemon_binary};
use whirlui_client::protocol::ErrorCode;
use whirlui_client::{Action, Client, ClientError, Event, Subscription, Update};

/// Say why a test did nothing, so a skipped run is visible rather than silent.
fn no_daemon() -> bool {
    if daemon_binary().is_none() {
        eprintln!(
            "skipped: no whirld. Point WHIRL_DAEMON at one, or build one into \
             target/whirl-daemon/debug/, to run the real-daemon tests."
        );
        return true;
    }
    false
}

/// Whether a pid is still a live process. `kill -0` is the portable check.
fn alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .expect("kill -0")
        .success()
}

/// The value of a `key: value` line in a response body.
fn value<'a>(lines: &'a [String], key: &str) -> Option<&'a str> {
    lines
        .iter()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
}

/// The greeting is read before anything is written, and `hello` is what follows
/// it (2.3, 2.4). Reaching the line after `connect_to` is the evidence: that call
/// reads the greeting, refuses a protocol this client does not know, sends
/// `hello` and reads `protocol: <m>` back, or it returns an error.
#[test]
fn negotiates_the_greeting_and_hello() {
    if no_daemon() {
        return;
    }
    let daemon = Daemon::start();
    let client = Client::connect_to(&daemon.socket()).expect("a negotiated client");
    assert_eq!(client.greeting().product, "whirl");
    assert_eq!(client.greeting().protocol, 2);
    assert!(whirlui_client::supports(client.greeting().protocol));
    client.close().expect("close");
}

/// The stable key set of 2.10, as a real daemon prints it.
#[test]
fn reads_status() {
    if no_daemon() {
        return;
    }
    let daemon = Daemon::start();
    daemon.plant("first.png", 1920, 1080);
    let mut client = Client::connect_to(&daemon.socket()).expect("a negotiated client");

    let status = client.status().expect("status");
    assert_eq!(status.get("daemon_version"), Some("whirl 0.1.0"));
    assert_eq!(status.protocol(), Some(2));
    assert_eq!(status.get("favorites_degraded"), Some("0"));
    assert!(!status.paused(), "{status:?}");
    assert!(!status.rotating(), "{status:?}");
    assert_eq!(status.number("rotation_count"), Some(0));
    assert!(status.seq() >= 1, "a fresh daemon has swept its cache once");
    assert!(status.sources_enabled() >= 1, "{status:?}");
    assert!(!status.favorites_degraded(), "{status:?}");
    assert!(
        status.lines().len() > 40,
        "status carries the whole key set: {}",
        status.lines().len()
    );
    // The keys a frontend reads by name are typed, and one that is absent reads
    // as unset rather than as a parse failure.
    assert_eq!(status.number("not_a_key"), None);
    client.close().expect("close");
}

/// Every verb milestone 1 needs, answered by a real daemon through this client's
/// own call path. Nothing here builds a request by hand: the point is that the
/// crate's encoding is the one the daemon reads.
#[test]
fn the_verbs_milestone_1_needs_are_answered_by_a_real_daemon() {
    if no_daemon() {
        return;
    }
    let daemon = Daemon::start();
    // Two images the daemon will admit: 1920x1080 and 2000x1000 both clear the
    // default 1600x900 bounds, and a header is all this build reads. Their
    // dimensions differ on purpose, because identical bytes are an identical
    // digest and a history of one digest is not a history.
    daemon.plant("first.png", 1920, 1080);
    let second = daemon.plant("second.png", 2000, 1000);

    let mut client = Client::connect_to(&daemon.socket()).expect("a negotiated client");

    // `sources` (2.11): one record per configured source, with its own words.
    let sources = client.sources().expect("sources");
    assert_eq!(sources.count, 1, "{sources:?}");
    assert_eq!(sources.records.len(), 1, "{sources:?}");
    assert_eq!(sources.records[0].id, "pictures", "{sources:?}");
    assert_eq!(sources.records[0].kind, "local", "{sources:?}");

    // `config path` (2.11 A): the file the daemon read, absolute.
    let config = client.config_path().expect("config path");
    assert!(Path::new(&config).is_absolute(), "{config}");
    assert_eq!(Path::new(&config), daemon.config().as_path());

    // `config check` (2.11): the plan the daemon adopted, which is what a
    // settings pane shows rather than the file's text.
    let check = client.config_check().expect("config check");
    assert!(!check.lines.is_empty(), "{check:?}");
    assert!(!check.records.is_empty(), "{check:?}");

    // `version` (2.11 A) and `ping` (2.5).
    let version = client.version().expect("version");
    assert_eq!(value(&version, "daemon_version"), Some("whirl 0.1.0"));
    assert_eq!(value(&version, "protocol"), Some("2"));
    client.ping().expect("ping");

    // `prev` with nothing behind it is a typed refusal, not a transport failure
    // (2.5, 2.7): the ring is empty before the first rotation.
    match client.prev() {
        Err(ClientError::Refused(refusal)) => {
            assert_eq!(refusal.code, ErrorCode::NoPrev, "{refusal}");
        }
        other => panic!("an empty history is `no_prev`, got {other:?}"),
    }

    // A rotation, `next` (2.5): `queued` first, then the `set:` record. The
    // terminator is not a data line, so it is not in this list.
    let lines = client.next().expect("next");
    assert_eq!(
        lines.first().map(String::as_str),
        Some("queued"),
        "{lines:?}"
    );
    let set = lines
        .iter()
        .find_map(|line| line.strip_prefix("set: "))
        .expect("a set line");
    let mut fields = set.split(' ');
    let digest = fields.next().expect("a digest").to_string();
    assert_eq!(digest.len(), 64, "a content digest: {digest}");
    assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()), "{digest}");

    let rotated = client.status().expect("status after a rotation");
    assert_eq!(rotated.get("last_digest"), Some(digest.as_str()));
    assert_eq!(rotated.number("rotation_count"), Some(1));

    // A second rotation, and then a `prev` that has somewhere to go. On this
    // build a `prev` that reaches an earlier image from a local source is a
    // typed refusal rather than a failure: the source references files in place,
    // so the digest is not in the cache and there is no route back from the
    // origin key to the bytes. What this client owes its caller is the code and
    // section 8 item 5's action for it, not a crash; a daemon that can walk back
    // answers with the `set:` record instead, and both are asserted here.
    client.next().expect("a second next");
    match client.prev() {
        Ok(lines) => {
            assert!(
                lines.iter().any(|line| line.starts_with("set: ")),
                "{lines:?}"
            );
        }
        Err(ClientError::Refused(refusal)) => {
            assert_eq!(refusal.code, ErrorCode::NotFound, "{refusal}");
            assert_eq!(
                refusal.action(),
                Some(Action::ReReadStatus),
                "`not_found` is section 8 item 5's \"re-read status\": {refusal}"
            );
        }
        Err(other) => panic!("prev: {other}"),
    }
    let after = client.status().expect("status after prev");
    assert!(
        after.number("rotation_count").unwrap_or(0) >= 2,
        "{after:?}"
    );
    assert!(!after.rotating(), "{after:?}");

    // `set path` (2.5): the user's own file, referenced and never copied.
    let lines = client
        .set_path(second.to_str().expect("a utf-8 path"))
        .expect("set path");
    let set = lines
        .iter()
        .find_map(|line| line.strip_prefix("set: "))
        .expect("a set line");
    let fields: Vec<&str> = set.split(' ').collect();
    assert_eq!(
        fields.len(),
        4,
        "set: <digest> <origin_key> <via> <path>: {set}"
    );
    assert_eq!(
        fields[2], "manual",
        "a `set path` is `via: manual` (2.6): {set}"
    );
    assert_eq!(
        fields[3],
        second.to_str().unwrap(),
        "the file is referenced: {set}"
    );
    let (manual_digest, manual_origin) = (fields[0].to_string(), fields[1].to_string());
    assert!(
        manual_origin.starts_with("external:"),
        "a hand-set file's origin is `external:<sha256 of the path>` (2.6): {set}"
    );
    assert_ne!(
        manual_digest, digest,
        "the two planted images are different bytes"
    );

    // `set id` (2.5) by the id the `set path` answer named. This build resolves
    // an id to a digest only when the digest is in its cache, and a test that
    // downloads nothing has an empty one, so the answer is a typed `not_found`
    // rather than a set: section 8 item 5's action for it is "re-read status",
    // and that mapping is the assertion. A daemon that can resolve the id
    // answers with the `set:` record, and that arm asserts it.
    match client.set_id(&manual_origin) {
        Ok(lines) => {
            assert!(
                lines.iter().any(|line| line.starts_with("set: ")),
                "{lines:?}"
            );
        }
        Err(ClientError::Refused(refusal)) => {
            assert_eq!(refusal.code, ErrorCode::NotFound, "{refusal}");
            assert_eq!(refusal.action(), Some(Action::ReReadStatus), "{refusal}");
        }
        Err(other) => panic!("set id: {other}"),
    }

    // `pause`, `resume` (2.5), with the flag read back rather than assumed.
    client.pause().expect("pause");
    let paused = client.status().expect("status while paused");
    assert!(paused.paused(), "pause did not take: {paused:?}");
    client.resume().expect("resume");
    let live = client.status().expect("status after resume");
    assert!(!live.paused(), "resume did not take: {live:?}");

    // `favorite`, `favorites`, `unfavorite` (2.5, 2.6). The pin's own answer
    // carries the id `unfavorite` takes, so the id is never guessed.
    let lines = client.favorite(None).expect("favorite");
    let favorited = lines
        .iter()
        .find_map(|line| line.strip_prefix("favorited: "))
        .expect("a favorited line");
    let fields: Vec<&str> = favorited.split(' ').collect();
    assert_eq!(
        fields.len(),
        2,
        "favorited: <digest> <origin_key>: {favorited}"
    );
    assert_eq!(fields[0].len(), 64, "a content digest: {favorited}");
    let origin_key = fields[1].to_string();

    let favorites = client.favorites().expect("favorites");
    assert_eq!(value(&favorites, "count"), Some("1"), "{favorites:?}");

    let lines = client.unfavorite(&origin_key).expect("unfavorite");
    assert!(
        lines.iter().any(|line| line.starts_with("unfavorited: ")),
        "{lines:?}"
    );
    let favorites = client.favorites().expect("favorites after unfavorite");
    assert_eq!(value(&favorites, "count"), Some("0"), "{favorites:?}");

    // `history` (2.6), newest first: the two rotations and the manual set are in
    // it, with the most recent change as the first record.
    let history = client.history(50).expect("history");
    let count: usize = value(&history, "count")
        .expect("a count")
        .parse()
        .expect("a number");
    assert!(count >= 3, "the ring holds what happened: {history:?}");
    let entries: Vec<&String> = history
        .iter()
        .filter(|line| line.starts_with("entry: "))
        .collect();
    assert_eq!(
        entries.len(),
        count,
        "one `entry:` line per count: {history:?}"
    );
    assert!(
        entries[0].contains(" manual "),
        "newest first, and the last change was the manual set: {}",
        entries[0]
    );

    // The ring is bounded by the daemon, not by the caller (2.6).
    let one = client.history(1).expect("history 1");
    assert_eq!(value(&one, "count"), Some("1"), "{one:?}");
    match client.history(51) {
        Err(ClientError::Refused(refusal)) => {
            assert_eq!(refusal.code, ErrorCode::BadArgs, "{refusal}");
        }
        other => panic!("history past its bound: {other:?}"),
    }

    // `close` (2.3): the client says it is finished.
    client.close().expect("close");
}

/// The daemon's events, through the subscription, with the snapshot the app
/// starts from.
#[test]
fn a_real_subscription_hands_back_the_daemons_events() {
    if no_daemon() {
        return;
    }
    let daemon = Daemon::start();
    daemon.plant("first.png", 1920, 1080);

    // `since: None`: the first update is the snapshot, so a frontend can draw
    // before anything happens (2.9, 8 item 3).
    let mut subscription = Subscription::open_at(&daemon.socket(), None).expect("a subscription");
    match subscription.next() {
        Update::Status(status) => {
            assert_eq!(status.get("daemon_version"), Some("whirl 0.1.0"));
            assert!(!status.paused(), "{status:?}");
        }
        other => panic!("the first update is the snapshot, got {other:?}"),
    }

    // Something happens on the command connection, and the subscription reports
    // it: this is how the app learns about changes without polling.
    subscription.client().pause().expect("pause");
    assert_eq!(
        wait_for(&mut subscription, |event| matches!(event, Event::Paused)),
        Update::Event(Event::Paused)
    );
    subscription.client().resume().expect("resume");
    assert_eq!(
        wait_for(&mut subscription, |event| matches!(event, Event::Resumed)),
        Update::Event(Event::Resumed)
    );

    // The snapshot stays current: a re-read is a status the app can draw.
    let paused = subscription.status().expect("a snapshot").paused();
    assert!(!paused, "the snapshot is the state after resume: {paused}");
}

/// The next update that is an event of a kind the caller names.
///
/// A daemon heartbeats every 30 s, and a heartbeat consumes a `seq` like any
/// other event (2.9), so a caller waiting for a particular event has to pass
/// over the ones it did not ask for instead of assuming it will be next.
fn wait_for(subscription: &mut Subscription, wanted: impl Fn(&Event) -> bool) -> Update {
    for _ in 0..16 {
        let update = subscription.next();
        match &update {
            Update::Event(event) if wanted(event) => return update,
            Update::Event(Event::Heartbeat { .. }) => continue,
            other => panic!("waiting for an event, got {other:?}"),
        }
    }
    panic!("the event never arrived");
}

/// A daemon that has been stopped is unreachable, which is a state a frontend
/// draws and not an error it reports.
#[test]
fn a_daemon_that_is_gone_is_unreachable() {
    if no_daemon() {
        return;
    }
    let daemon = Daemon::start();
    let socket = daemon.socket();
    // The client agrees the daemon is there before it is taken away, so the
    // difference afterwards is the daemon and not the path.
    Client::connect_to(&socket).expect("a daemon that is running");
    drop(daemon);

    let error = Client::connect_to(&socket).expect_err("a socket with no daemon behind it");
    assert!(error.unreachable(), "{error}");
    let text = error.to_string();
    assert!(text.contains("w.sock"), "{text}");
    assert!(
        !text.contains(&socket.display().to_string().replace("/w.sock", "")),
        "{text}"
    );
}

/// No test leaves a daemon or a socket behind.
#[test]
fn nothing_the_test_started_outlives_it() {
    if no_daemon() {
        return;
    }
    let daemon = Daemon::start();
    let socket: PathBuf = daemon.socket();
    let dir: PathBuf = daemon.dir().to_path_buf();
    let pid = daemon.pid();
    assert!(socket.exists(), "the daemon is listening");
    assert!(alive(pid), "the daemon is running");

    drop(daemon);

    assert!(!socket.exists(), "the socket file was left behind");
    assert!(!dir.exists(), "the daemon's scratch tree was left behind");
    assert!(!alive(pid), "the daemon process was left behind");
}
