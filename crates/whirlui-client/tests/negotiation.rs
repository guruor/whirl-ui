//! The negotiation and the recovery rules, against a scripted daemon.
//!
//! These run everywhere, with no daemon installed, because they are about what a
//! frontend does when the far side is not a healthy daemon: a version it cannot
//! speak, a socket that is not there, a socket nothing is listening on, a gap, a
//! restart, and silence. `daemon.rs` covers the same rules against a real
//! `whirld` where one is available.
#![cfg(unix)]

mod support;

use std::os::unix::net::UnixListener;
use std::time::Duration;

use support::{Reply, Scratch, Stub, hello_reply, ok_after, ok_reply, stub_status_lines};
use whirlui_client::{Client, ClientError, Event, Subscription, Update};

/// What a healthy daemon calls itself. 2.4: `OK <product> <version> protocol
/// <n>`, the name once and the version bare.
const GREETING: &str = "OK whirl 0.1.0 protocol 2";

#[test]
fn refuses_an_unknown_protocol_version() {
    // The refusal has to happen before anything is written: 2.3 step 1 is "read
    // the greeting before writing", and a client that speaks the wrong protocol
    // can only make things worse by talking. Removing the version check in
    // `Client::connect_to` makes this test fail: the client writes `hello`, a
    // scripted daemon that answers nothing leaves it there, and `expect_err` sees
    // a `WouldBlock` after the 300 s command timeout instead of an unknown
    // protocol.
    let stub = Stub::start("OK whirl 9.9.9 protocol 3", |_line, _stream| Reply::Silence);

    let error = Client::connect_to(stub.socket()).expect_err("a protocol this client cannot speak");
    match error {
        ClientError::UnknownProtocol { ref server, .. } => assert_eq!(server, "3"),
        other => panic!("expected an unknown protocol, got {other:?}"),
    }
    assert_eq!(
        stub.received(),
        "",
        "the greeting is read before anything is written"
    );
}

#[test]
fn an_absent_socket_is_unreachable_and_names_no_directory() {
    let scratch = Scratch::new("absent");
    let path = scratch.join("w.sock");

    let error = Client::connect_to(&path).expect_err("a socket that is not there");
    assert!(error.unreachable(), "{error}");
    match error {
        ClientError::Unreachable(ref socket) => {
            assert_eq!(socket.name, "w.sock");
            assert_eq!(socket.mode, None);
            assert!(!socket.stale());
        }
        other => panic!("expected an unreachable socket, got {other:?}"),
    }
    // The diagnostic names the socket, not the account it runs under.
    let text = error.to_string();
    assert!(text.contains("w.sock"), "{text}");
    assert!(
        !text.contains(&scratch.path().display().to_string()),
        "{text}"
    );
}

#[test]
fn a_stale_socket_is_reported_and_left_alone() {
    let scratch = Scratch::new("stale");
    let path = scratch.join("w.sock");
    // A daemon that died without unlinking: the file is there, and nothing is
    // listening on it.
    let listener = UnixListener::bind(&path).expect("a bound socket");
    drop(listener);

    let error = Client::connect_to(&path).expect_err("a socket with nothing behind it");
    match error {
        ClientError::Unreachable(ref socket) => {
            assert!(socket.stale(), "{socket}");
            assert!(socket.mode.is_some(), "{socket}");
        }
        other => panic!("expected a stale socket, got {other:?}"),
    }
    // Section 8, "must never" 5: the socket is the daemon's to remove. Removing
    // the version check above does not affect this; removing the refusal to
    // unlink would.
    assert!(path.exists(), "the client unlinked the socket");
}

#[test]
fn re_reads_status_after_a_seq_gap() {
    // A daemon whose `seq` is 2, asked to resume from 0: everything between is
    // gone, and one event follows so that a client which ignored the gap has
    // something to hand back instead of a snapshot. Dropping the daemon's `gap:`
    // line on the floor in `EventStream::open_at` makes this test fail, with
    // `Event(Paused)` where the snapshot should be.
    let stub = Stub::start(GREETING, |line, _stream| match line {
        "subscribe 0" => Reply::Lines(vec![
            "subscribed: 2".to_string(),
            "gap: 2".to_string(),
            // The event the daemon would send next, which is the answer a client
            // that skipped the gap would return.
            "event: 3 paused".to_string(),
        ]),
        line if line.starts_with("subscribe") => Reply::Lines(vec!["subscribed: 2".to_string()]),
        line if line.starts_with("hello") => Reply::Lines(hello_reply()),
        "status" => Reply::Lines(ok_after(stub_status_lines(true, 2))),
        "close" => Reply::Lines(ok_reply()),
        other => Reply::Lines(vec![format!("ERR bad_args {other}")]),
    });

    let mut subscription =
        Subscription::open_at(stub.socket(), Some(0)).expect("a subscription that resumes");
    match subscription.next() {
        Update::Status(status) => {
            assert!(status.paused(), "the status was not re-read: {status:?}");
            assert_eq!(status.seq(), 2);
        }
        other => panic!("a gap must be answered with a re-read status, got {other:?}"),
    }
}

#[test]
fn a_resume_point_ahead_of_the_daemon_is_a_restart_and_re_reads_status() {
    // `seq` starts at 1 per run and is never persisted (2.9), so a daemon
    // reporting 1 to a client that saw 9 is a different daemon run. The event is
    // there so that a client which ignored the restart has something to hand
    // back instead of a snapshot.
    let stub = Stub::start(GREETING, |line, _stream| match line {
        "subscribe 9" => Reply::Lines(vec![
            "subscribed: 1".to_string(),
            "event: 2 paused".to_string(),
        ]),
        line if line.starts_with("subscribe") => Reply::Lines(vec!["subscribed: 1".to_string()]),
        line if line.starts_with("hello") => Reply::Lines(hello_reply()),
        "status" => Reply::Lines(ok_after(stub_status_lines(true, 1))),
        "close" => Reply::Lines(ok_reply()),
        other => Reply::Lines(vec![format!("ERR bad_args {other}")]),
    });

    let mut subscription = Subscription::open_at(stub.socket(), Some(9)).expect("a subscription");
    match subscription.next() {
        Update::Status(status) => assert_eq!(status.seq(), 1),
        other => panic!("a restart must be answered with a re-read status, got {other:?}"),
    }
}

#[test]
fn a_gapless_subscription_hands_back_events_and_no_snapshot() {
    let stub = Stub::start(GREETING, |line, _stream| match line {
        "subscribe 5" => Reply::Lines(vec![
            "subscribed: 5".to_string(),
            "event: 6 resumed".to_string(),
        ]),
        line if line.starts_with("subscribe") => Reply::Lines(vec!["subscribed: 5".to_string()]),
        line if line.starts_with("hello") => Reply::Lines(hello_reply()),
        "close" => Reply::Lines(ok_reply()),
        other => Reply::Lines(vec![format!("ERR bad_args {other}")]),
    });

    let mut subscription = Subscription::open_at(stub.socket(), Some(5)).expect("a subscription");
    assert_eq!(subscription.next(), Update::Event(Event::Resumed));
}

#[test]
fn silence_reconnects_and_a_daemon_that_is_gone_is_offline() {
    let mut stub = Stub::start(GREETING, |line, _stream| match line {
        line if line.starts_with("subscribe") => Reply::Lines(vec!["subscribed: 1".to_string()]),
        line if line.starts_with("hello") => Reply::Lines(hello_reply()),
        "status" => Reply::Lines(ok_after(stub_status_lines(false, 1))),
        "close" => Reply::Lines(ok_reply()),
        other => Reply::Lines(vec![format!("ERR bad_args {other}")]),
    });

    let mut subscription = Subscription::open_at(stub.socket(), Some(1)).expect("a subscription");
    // Five missed heartbeats at this scale are five of these.
    subscription
        .set_liveness(Duration::from_millis(300))
        .expect("a liveness window");
    subscription.set_reconnect_delay(Duration::ZERO);

    // Nothing arrives, so the daemon is gone; the client reconnects and re-reads
    // `status`, exactly as section 8 item 4 says it must.
    match subscription.next() {
        Update::Status(status) => assert_eq!(status.seq(), 1),
        other => panic!("silence must be answered with a re-read status, got {other:?}"),
    }

    // And once there is nothing to reconnect to, the caller is told the daemon
    // is not there rather than being handed an error or a stale snapshot.
    stub.stop();
    assert_eq!(subscription.next(), Update::Offline);
    assert_eq!(subscription.next(), Update::Offline);
}

#[test]
fn a_connection_that_is_never_answered_fails_rather_than_hanging() {
    // The command connection waits 2.8's 300 s for an answer; the test does not.
    let stub = Stub::start(GREETING, |line, _stream| match line {
        line if line.starts_with("hello") => Reply::Lines(hello_reply()),
        // `status` is never answered at all.
        _ => Reply::Silence,
    });

    let mut client = Client::connect_to(stub.socket()).expect("a negotiated client");
    client
        .set_read_timeout(Duration::from_millis(200))
        .expect("a read timeout");
    let error = client.status().expect_err("an answer that never came");
    assert!(error.timed_out(), "{error}");
}
