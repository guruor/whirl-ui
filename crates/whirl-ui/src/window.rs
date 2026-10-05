//! The settings window as a window: the activation policy it puts the app
//! under, and the raise a second `Settings…` performs.
//!
//! A menu bar item with a dialog is two things at once, and macOS has one
//! switch for both. An agent app -- `LSUIElement` in the bundle, and the
//! accessory policy the event loop is built with -- has no Dock tile, no
//! Cmd+Tab entry and windows the window server does not manage. That is what a
//! tray-only app wants, and it is also why the settings window could not be
//! found again: with the window behind another app nothing could raise it, and
//! the app was not one of the ones Cmd+Tab switches between.
//!
//! So the policy follows the window. [`policy_for`] is the whole rule: accessory
//! while nothing is on screen -- M1 criterion 5's no-permanent-Dock-tile rule is
//! what that half keeps -- and regular while a window is, which is what gives
//! the app a Dock tile, a Cmd+Tab entry and a window the window manager manages.
//!
//! And a `Settings…` click with the window already up is a raise, not an
//! activation. [`raise`] is the whole of it, and it is four window operations in
//! an order: order the window to the front, make it key and main, take it out of
//! the Dock's minimised state when it is there, and only then bring the app
//! forward. Activating the app alone is what the row did before and it is the
//! defect: the desktop comes forward with a window that is still behind
//! everything else.
//!
//! Both live behind a seam a test can drive -- the rule as a value, the raise as
//! a trait that the AppKit implementation and a recorder both satisfy -- because
//! the property under test is what the app asks for and what the raise does, and
//! a headless test cannot put either question to the window server.

#![cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the tray is the only caller, and the tray is macOS-only"
    )
)]

/// What the app asks macOS for, and it changes with the window.
///
/// The two values are the whole of the rules an agent app trades in: no Dock
/// tile and nothing in the switcher, or both. There is no third state here,
/// because the app is never a background-only process: it has a status item,
/// which needs an accessory policy at least.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Nothing on screen: no Dock tile and no Cmd+Tab entry. This is a menu bar
    /// item's own state, and it is what M1 criterion 5 asks for.
    Accessory,
    /// A window on screen: a Dock tile, a Cmd+Tab entry, and a window the
    /// window manager manages, so it can be raised, minimised and switched to
    /// like any other app's.
    Regular,
}

/// The policy a window state asks for.
///
/// The one place the conditional rule is decided, so the two callers -- the pass
/// that shows the window and the pass that hides it -- cannot disagree about it,
/// and so the test asserts the rule rather than the call sites.
pub fn policy_for(window_open: bool) -> Policy {
    if window_open {
        Policy::Regular
    } else {
        Policy::Accessory
    }
}

/// One step of a raise, in the order it happens.
///
/// A raise is reported as the steps it took rather than as one "raised" flag,
/// because the difference between this and what the row did before is a
/// difference in the steps: activation without a key, front window is the
/// defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Order the window in front of its siblings.
    Front,
    /// Make it the key and main window of the app.
    KeyAndMain,
    /// Take it out of the Dock's minimised state.
    Unminimise,
    /// Bring the app itself forward, last.
    Activate,
}

/// The window operations a raise performs.
///
/// A trait rather than four free functions, so that a test can drive a raise
/// with a recorder and assert the calls it made, and so that the AppKit calls
/// are the only platform code behind it. Each method answers whether it was
/// performed at all, which is how the report below stays a report: a raise that
/// found no window says so by taking no steps, rather than claiming steps that
/// did not happen.
pub trait Window {
    /// Whether the window is in the Dock's minimised state.
    fn minimised(&self) -> bool;
    /// Order the window in front of its siblings.
    fn order_front(&self) -> bool;
    /// Make it the app's key and main window.
    fn make_key_and_main(&self) -> bool;
    /// Take it out of the Dock's minimised state.
    fn unminimise(&self) -> bool;
    /// Bring the app itself forward.
    fn activate_app(&self) -> bool;
}

/// Raise the window, and report the steps that were taken.
///
/// The order is the whole point and it is why this is a function rather than
/// four calls in the click handler: the window is put in front and made key
/// before the app is activated, so the app coming forward cannot present a
/// window that is still behind its siblings. Activating the app and nothing else
/// is the defect this replaces.
pub fn raise(window: &dyn Window) -> Vec<Step> {
    let mut took = Vec::new();
    if window.order_front() {
        took.push(Step::Front);
    }
    if window.make_key_and_main() {
        took.push(Step::KeyAndMain);
    }
    if window.minimised() && window.unminimise() {
        took.push(Step::Unminimise);
    }
    if window.activate_app() {
        took.push(Step::Activate);
    }
    took
}

/// What a `Settings…` click does, given whether a window is already up, and the
/// whole of the rule that a second click raises rather than opens.
///
/// `None` is "there is no window, the caller opens one": the click with nothing
/// on screen is a read and a first draw, and there is nothing to raise. `Some`
/// is the already-open arm, and the steps are what the raise did -- never a
/// second window, and never a bare activation.
///
/// The panes the click read are deliberately not part of this: the already-open
/// arm drops them rather than putting them on screen, because replacing the
/// dialog's state would throw away whatever is half-edited in it.
pub fn settings_asked(window_open: bool, window: &dyn Window) -> Option<Vec<Step>> {
    window_open.then(|| raise(window))
}

/// The AppKit side: the policy call, and the window a raise acts on.
///
/// Raw AppKit rather than a viewport command, because neither of the two things
/// this card is about is expressible as one: the activation policy has no eframe
/// surface at all (winit sets it once, from the event loop builder), and
/// `makeKeyAndOrderFront` is a request about the window the window server keeps,
/// not about what egui draws. The bindings are the same `objc2` ones `tray-icon`
/// is built on, so this is one way into AppKit and not two.
#[cfg(target_os = "macos")]
mod appkit {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::NSString;

    use super::{Policy, Window};
    use crate::settings::WINDOW_TITLE;

    /// The app's settings window, as AppKit has it.
    ///
    /// Found by the title eframe gave it rather than by its place in the list:
    /// there is one window today, and a list order is not something to build a
    /// raise on. `mainWindow` is the second look, for the pass that runs before
    /// the title has settled.
    fn settings_window() -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        let wanted = NSString::from_str(WINDOW_TITLE);
        app.windows()
            .iter()
            .find(|window| window.title().isEqualToString(&wanted))
            .or_else(|| app.mainWindow())
    }

    /// Ask macOS for a policy. The one call the window's lifetime is stated by.
    ///
    /// `setActivationPolicy` answers whether the request was accepted; there is
    /// nothing this app can do about a `false` -- it would be a macOS that
    /// refuses to move the app between the two policies -- so it is not read
    /// back. What the policy *is* is quoted from the system in the card's
    /// summary rather than asserted here.
    pub fn apply(policy: Policy) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let wanted = match policy {
            Policy::Accessory => NSApplicationActivationPolicy::Accessory,
            Policy::Regular => NSApplicationActivationPolicy::Regular,
        };
        app.setActivationPolicy(wanted);
    }

    /// The window a raise acts on, through the raw AppKit calls.
    ///
    /// A unit type: the window is looked up on each call rather than held,
    /// because holding a `Retained<NSWindow>` across frames would be this module
    /// remembering something AppKit already knows, and the lookup is a walk of a
    /// one-element list.
    pub struct AppKit;

    impl Window for AppKit {
        fn minimised(&self) -> bool {
            settings_window().is_some_and(|window| window.isMiniaturized())
        }

        fn order_front(&self) -> bool {
            let Some(window) = settings_window() else {
                return false;
            };
            window.orderFront(None);
            true
        }

        fn make_key_and_main(&self) -> bool {
            let Some(window) = settings_window() else {
                return false;
            };
            window.makeKeyAndOrderFront(None);
            true
        }

        fn unminimise(&self) -> bool {
            let Some(window) = settings_window() else {
                return false;
            };
            window.deminiaturize(None);
            true
        }

        fn activate_app(&self) -> bool {
            let Some(mtm) = MainThreadMarker::new() else {
                return false;
            };
            let app = NSApplication::sharedApplication(mtm);
            // `-[NSApplication activate]` is macOS 14, and the bundle runs from
            // macOS 13 (`LSMinimumSystemVersion` in scripts/make-bundle.sh), so
            // the older spelling is the one to call: a selector that does not
            // exist on the older system is a crash, not a fallback. It is
            // deprecated, not removed.
            #[allow(
                deprecated,
                reason = "`activate` needs macOS 14; the bundle runs from 13"
            )]
            {
                app.activateIgnoringOtherApps(true);
            }
            true
        }
    }
}

#[cfg(target_os = "macos")]
pub use appkit::{AppKit, apply};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// A window that records every operation a raise makes, so the assertion is
    /// on the calls rather than on a report the raise wrote for itself.
    #[derive(Debug, Default)]
    struct Recorded {
        minimised: bool,
        calls: RefCell<Vec<Step>>,
    }

    impl Recorded {
        fn calls(&self) -> Vec<Step> {
            self.calls.borrow().clone()
        }

        fn record(&self, step: Step) -> bool {
            self.calls.borrow_mut().push(step);
            true
        }
    }

    impl Window for Recorded {
        fn minimised(&self) -> bool {
            self.minimised
        }

        fn order_front(&self) -> bool {
            self.record(Step::Front)
        }

        fn make_key_and_main(&self) -> bool {
            self.record(Step::KeyAndMain)
        }

        fn unminimise(&self) -> bool {
            self.record(Step::Unminimise)
        }

        fn activate_app(&self) -> bool {
            self.record(Step::Activate)
        }
    }

    #[test]
    fn the_policy_is_regular_exactly_while_a_window_is_open() {
        // The conditional rule, in both directions, which is what the assertion
        // this replaces could only state one way (see `main.rs`).
        assert_eq!(
            policy_for(false),
            Policy::Accessory,
            "no window: no Dock tile, and nothing in the switcher"
        );
        assert_eq!(
            policy_for(true),
            Policy::Regular,
            "a window on screen: a Dock tile, and an entry in the switcher"
        );
    }

    #[test]
    fn a_settings_click_with_a_window_up_raises_it_rather_than_activating_the_app() {
        // The already-open arm: the whole of what a second `Settings…` does.
        let window = Recorded::default();
        let took = settings_asked(true, &window).expect("the already-open arm raises");
        assert_eq!(
            took,
            window.calls(),
            "the report is the calls that were made"
        );
        assert_eq!(took.first(), Some(&Step::Front), "the window goes in front");
        assert!(
            took.contains(&Step::KeyAndMain),
            "and is made key and main: {took:?}"
        );
        assert_eq!(
            took.last(),
            Some(&Step::Activate),
            "and the app is brought forward last: {took:?}"
        );
        assert_ne!(
            took,
            vec![Step::Activate],
            "activating the app alone is the defect, not the fix"
        );

        // The closed arm opens a window; it raises nothing, because there is
        // nothing to raise and no second window to make.
        let closed = Recorded::default();
        assert!(settings_asked(false, &closed).is_none());
        assert!(
            closed.calls().is_empty(),
            "no window is up, so no raise happened"
        );
    }

    #[test]
    fn a_raise_unminimises_only_a_window_that_is_minimised() {
        // What makes the report a report: the step is taken from the window's
        // own state rather than listed whether or not it was needed.
        let minimised = Recorded {
            minimised: true,
            ..Recorded::default()
        };
        let took = settings_asked(true, &minimised).expect("raised");
        assert!(took.contains(&Step::Unminimise), "{took:?}");

        let on_screen = Recorded::default();
        let took = settings_asked(true, &on_screen).expect("raised");
        assert!(!took.contains(&Step::Unminimise), "{took:?}");
    }
}
