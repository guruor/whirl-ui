//! What the app knows, and how a `subscribe` event changes it.
//!
//! The menu renders from the last known state, and this module is that state.
//! One `status` when the app starts gives the complete snapshot (whirl's
//! `docs/architecture.md` 2.10), and the events that follow are notifications,
//! not data transfer (2.9), so an event is *applied* to the snapshot rather than
//! answered with another round trip. That is the whole reason `subscribe`
//! exists: opening the menu never asks the daemon anything, and no code here
//! polls.
//!
//! What an event may not do is invent a key. A rotation's new image arrives as
//! the `last_origin_key` and `last_digest` values of `rotate_ok` (2.9), which
//! are the same two names `status` uses, so the line the menu draws is the
//! daemon's own name for the image either way.

use whirlui_client::{Event, Status};

/// What `status` prints for a key it has no value for. Section 2.6: "a `-`
/// value means unset, never empty string", so `-` is never a name.
const UNSET: &str = "-";

/// What the menu draws right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// The daemon is not reachable. The item still appears, says so, and the
    /// rows that would need the daemon are disabled.
    Offline,
    /// The last snapshot, with the deltas the events have carried since.
    Live(Live),
}

/// A `status` snapshot plus the two things the events can change.
///
/// Only the keys the menu draws are tracked, and each one is tracked by the
/// name section 2.10 gives it. `status` itself is kept whole so the settings
/// window (M1 criterion 6) can read the keys the menu does not draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live {
    status: Status,
    paused: bool,
    /// The image the daemon last announced, named the way `status` names it.
    now: Option<String>,
}

impl Live {
    /// A snapshot as the daemon printed it.
    pub fn new(status: Status) -> Live {
        let paused = status.paused();
        let now = named(&status, "last_origin_key").or_else(|| named(&status, "last_digest"));
        Live {
            status,
            paused,
            now,
        }
    }

    /// Whether the schedule is suspended, as `paused` says (2.5).
    pub fn paused(&self) -> bool {
        self.paused
    }
}

impl View {
    /// The daemon is not reachable.
    pub fn offline() -> View {
        View::Offline
    }

    /// A view of one snapshot.
    pub fn live(status: Status) -> View {
        View::Live(Live::new(status))
    }

    /// The snapshot, when the app has one.
    pub fn snapshot(&self) -> Option<&Live> {
        match self {
            View::Offline => None,
            View::Live(live) => Some(live),
        }
    }

    /// Whether the daemon answered at all.
    pub fn reachable(&self) -> bool {
        self.snapshot().is_some()
    }

    /// Whether the schedule is suspended.
    pub fn paused(&self) -> bool {
        self.snapshot().is_some_and(Live::paused)
    }

    /// Whether pin state is unknown, so the pin row must be hidden rather than
    /// offered (section 8 item 5: `favorites_degraded` means "hide the pin
    /// affordance").
    pub fn favourites_degraded(&self) -> bool {
        self.snapshot()
            .is_some_and(|live| live.status.favorites_degraded())
    }

    /// The current-image line.
    ///
    /// It is the daemon's own name for the image, from the stable key set of
    /// 2.10: `last_origin_key` when there is one (`space:ab12cd` on a remote
    /// source, `<source>:<scoped id>` on a local one), the content digest when
    /// there is not, and a sentence when nothing has been set yet. It is never
    /// a file path, so the line carries no home directory and no cache layout
    /// into a screenshot.
    pub fn image_line(&self) -> String {
        match self.snapshot() {
            None => "the daemon is not running".to_string(),
            Some(live) => live
                .now
                .clone()
                .unwrap_or_else(|| "no image set yet".to_string()),
        }
    }

    /// Apply one event, which is section 8 item 3's rule read the other way
    /// round: a gapless event *is* the new state, so nothing has to be re-read.
    ///
    /// The tray's event thread is the only caller and the tray is macOS-only for
    /// now, so on the other two legs a view never changes after it is read. The
    /// tests below are then the only thing that exercises this, which is the
    /// point of keeping it here rather than in the platform-gated module.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn apply(&mut self, event: &Event) {
        let View::Live(live) = self else {
            // An event without a snapshot is a state this app does not hold:
            // the subscription re-reads `status` after a gap, a restart or a
            // silence before it hands over any event.
            return;
        };
        match event {
            Event::RotateOk {
                digest, origin_key, ..
            } => {
                live.now = Some(if origin_key.is_empty() || origin_key == UNSET {
                    digest.clone()
                } else {
                    origin_key.clone()
                });
            }
            Event::Paused => live.paused = true,
            Event::Resumed => live.paused = false,
            // Everything else is a fact the menu does not draw: a heartbeat, a
            // sweep, a rotation that failed, the daemon going away (which ends
            // the stream and comes back as `Update::Offline`). None of them is a
            // reason to change a row.
            _ => {}
        }
    }
}

/// The value of `key`, when the daemon gave it a name rather than a `-`.
fn named(status: &Status, key: &str) -> Option<String> {
    status
        .get(key)
        .filter(|value| !value.is_empty() && *value != UNSET)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `status` body with the keys the menu reads, and the rest of 2.10's
    /// shape around them.
    fn status(origin: &str, digest: &str, paused: bool, degraded: bool) -> Status {
        let lines: Vec<String> = [
            "daemon_version: whirl 0.1.0".to_string(),
            "protocol: 2".to_string(),
            "platform: macos".to_string(),
            "seq: 7".to_string(),
            format!("paused: {}", u8::from(paused)),
            format!("favorites_degraded: {}", u8::from(degraded)),
            format!("last_digest: {digest}"),
            format!("last_origin_key: {origin}"),
            "anchor_path: /home/someone/Pictures/a.png".to_string(),
            "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
        ]
        .to_vec();
        Status::from_lines(&lines)
    }

    #[test]
    fn a_snapshot_names_the_image_by_its_origin_key() {
        let view = View::live(status("space:ab12cd", "d43584", false, false));
        assert_eq!(view.image_line(), "space:ab12cd");
        assert!(view.reachable());
        assert!(!view.paused());
        assert!(!view.favourites_degraded());
    }

    #[test]
    fn the_image_line_is_a_name_and_never_a_path() {
        let view = View::live(status("space:ab12cd", "d43584", false, false));
        let line = view.image_line();
        assert!(!line.contains('/'), "{line}");
        assert!(!line.contains("home"), "{line}");
        assert!(!line.contains(".png"), "{line}");
    }

    #[test]
    fn a_daemon_that_is_not_running_says_so() {
        let view = View::offline();
        assert!(!view.reachable());
        assert_eq!(view.image_line(), "the daemon is not running");
        assert!(!view.paused());
    }

    #[test]
    fn a_daemon_with_nothing_on_screen_says_that_too() {
        let view = View::live(status("-", "-", false, false));
        assert_eq!(view.image_line(), "no image set yet");
    }

    #[test]
    fn a_rotation_arrives_as_the_same_two_names_status_uses() {
        let mut view = View::live(status("space:ab12cd", "d43584", false, false));
        view.apply(&Event::RotateOk {
            digest: "beef".to_string(),
            origin_key: "pictures:99".to_string(),
            via: "source".to_string(),
            path: Some("/home/someone/Pictures/b.png".to_string()),
        });
        assert_eq!(view.image_line(), "pictures:99");
    }

    #[test]
    fn pause_and_resume_follow_the_events() {
        let mut view = View::live(status("space:ab12cd", "d43584", false, false));
        assert!(!view.paused());
        view.apply(&Event::Paused);
        assert!(view.paused());
        view.apply(&Event::Resumed);
        assert!(!view.paused());
    }

    #[test]
    fn a_heartbeat_changes_nothing() {
        let before = View::live(status("space:ab12cd", "d43584", true, false));
        let mut after = before.clone();
        after.apply(&Event::Heartbeat {
            unix_seconds: 1_791_005_820,
        });
        assert_eq!(before, after);
    }

    #[test]
    fn an_event_with_no_snapshot_is_ignored_rather_than_invented() {
        let mut view = View::offline();
        view.apply(&Event::Paused);
        assert_eq!(view, View::offline());
    }
}
