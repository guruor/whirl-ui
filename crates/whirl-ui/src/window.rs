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
//! And the app has one way out, whichever door is used. `⌘Q`, and every other
//! AppKit terminate, are answered by `applicationShouldTerminate:` rather than by
//! the tray's `Quit` row, so `install_terminate_handler` adds that method to the
//! delegate class and [`answer_terminate`] is what it answers with: run the app's
//! own quit flow, and never let AppKit carry the terminate out. That flow's own
//! close is what ends the process, so a terminate AppKit carried out would be a
//! second exit that skipped the question, the remembered answer and the report
//! line -- which is the exit `⌘Q` was.
//!
//! Each lives behind a seam a test can drive -- the rule as a value, the raise as
//! a trait that the AppKit implementation and a recorder both satisfy -- because
//! the property under test is what the app asks for and what the raise and the
//! terminate do, and a headless test cannot put any of those questions to the
//! window server.

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

/// What the app answers a terminate with, and the request that goes with the
/// answer.
///
/// AppKit asks once, through `applicationShouldTerminate:`, and the answer is
/// the one the tray's `Quit` row already gives: run *that* flow -- the question,
/// the remembered answer, the default of keeping the daemon running, and the
/// report line -- and refuse the terminate. The flow's own close is what ends the
/// process: the tray's own quit is what sends `ViewportCommand::Close`, once the
/// plan has run, and AppKit's terminate sends nothing. So a terminate AppKit
/// carried out would be a second exit that skipped all of it. That second exit
/// is what `⌘Q` was.
///
/// `request` is the app's own request and not a second one: the tray hands over
/// the same `request_quit` the row calls, so the two doors cannot drift apart.
///
/// The return value is whether AppKit may carry the terminate out, and it is
/// always `false`. It is a value rather than a comment so the test below asserts
/// the direction instead of reading a doc line: `true` here is the defect, and it
/// is one character wide.
pub fn answer_terminate(request: &dyn Fn()) -> bool {
    request();
    false
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
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSApplicationTerminateReply,
    };
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

    /// The request a terminate runs, once the handler is installed.
    ///
    /// A slot in the process rather than a field on something, because the method
    /// that reads it is a bare `extern "C"` function:
    /// `applicationShouldTerminate:` is added to the delegate class at runtime
    /// (`install_terminate_handler`), and a method added that way has no `self`
    /// of this app's to carry the request. It is filled before the method goes
    /// in, so a terminate cannot find it empty.
    static TERMINATE_REQUEST: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> =
        std::sync::OnceLock::new();

    /// The shape of the method, so the function below can be turned into an `Imp`.
    ///
    /// The two parameters are raw pointers rather than references because
    /// `objc2`'s `MethodImplementation` is not implemented for a function
    /// pointer that is higher-ranked over lifetimes -- `fn(&AnyObject, Sel,
    /// &AnyObject)` is, and it is rejected with "implementation of
    /// `MethodImplementation` is not general enough" -- while a raw pointer
    /// carries no lifetime and is. It is the same argument, spelled the one way
    /// the trait takes.
    type ShouldTerminate = unsafe extern "C-unwind" fn(
        *mut objc2::runtime::AnyObject,
        objc2::runtime::Sel,
        *mut objc2::runtime::AnyObject,
    ) -> NSApplicationTerminateReply;

    /// The app's answer to `applicationShouldTerminate:`: run the app's own quit
    /// flow, and refuse the terminate.
    ///
    /// `install_terminate_handler` adds this under the encoding
    /// `NSApplicationTerminateReply (id, SEL, NSApplication *)`, which is
    /// `"Q@:@"`: `NSApplicationTerminateReply` is an `NSUInteger`-backed
    /// `NS_ENUM`, and on a 64-bit target `usize` encodes as `Q` (`objc2`'s own
    /// `Encode for usize` picks `u64`). The two arguments are unused: the
    /// question is the app's, not the sender's.
    ///
    /// The arm with nothing installed answers `TerminateNow`, and it is
    /// unreachable while the method exists. It is there so that a terminate which
    /// somehow arrives with no flow behind it is a plain quit, rather than an app
    /// that will not close.
    unsafe extern "C-unwind" fn should_terminate(
        _this: *mut objc2::runtime::AnyObject,
        _cmd: objc2::runtime::Sel,
        _sender: *mut objc2::runtime::AnyObject,
    ) -> NSApplicationTerminateReply {
        match TERMINATE_REQUEST.get() {
            Some(request) if !super::answer_terminate(request.as_ref()) => {
                NSApplicationTerminateReply::TerminateCancel
            }
            _ => NSApplicationTerminateReply::TerminateNow,
        }
    }

    /// Put AppKit's terminate on the app's own quit flow.
    ///
    /// `applicationShouldTerminate:` is added to the running application's
    /// delegate class, because that delegate is winit's and not this app's:
    /// eframe gives the event loop no delegate of its own, and replacing winit's
    /// would take the loop's own machinery with it -- winit's delegate is where
    /// its `AppState` lives. winit answers `applicationDidFinishLaunching:` and
    /// `applicationWillTerminate:` and not this method, so without it AppKit's
    /// answer to `terminate:` is "now". That selector is what the main menu's
    /// `Quit` item sends on `⌘Q`, so the whole of the process ended with the
    /// question never asked: this is what was wrong with `⌘Q`.
    ///
    /// `request` is `tray::Shared::request_quit`, the request the row makes, so
    /// the two doors cannot drift apart. Returns whether *this call* opened the
    /// door: `false` means AppKit's own answer stands -- the delegate's class
    /// already answers the method, or there is no application to put it on -- and
    /// the caller should say so rather than assume a `⌘Q` now asks.
    pub fn install_terminate_handler(request: impl Fn() + Send + Sync + 'static) -> bool {
        use objc2::runtime::{AnyClass, AnyObject, Imp, MethodImplementation};
        use objc2::sel;

        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
            return false;
        };
        let object: &AnyObject = AsRef::<AnyObject>::as_ref(&*delegate);
        let method = sel!(applicationShouldTerminate:);
        let class = object.class();
        // One door per process: either the method is already there -- ours, or a
        // winit that has grown one -- or it goes in below.
        if class.instance_method(method).is_some() {
            return false;
        }
        if TERMINATE_REQUEST.set(Box::new(request)).is_err() {
            return false;
        }
        let imp: Imp = (should_terminate as ShouldTerminate).__imp();
        // SAFETY: `class` is the delegate's own class, and the method is one
        // AppKit itself calls on a delegate; `imp` is `should_terminate` above,
        // whose signature is the one the encoding declares. `class_addMethod` is
        // the runtime's call for exactly this: a method added to a class that is
        // already registered (`class_addIvar` is the one that needs it before).
        let added = unsafe {
            objc2::ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                method,
                imp,
                c"Q@:@".as_ptr(),
            )
        };
        added.as_bool()
    }
}

#[cfg(target_os = "macos")]
pub use appkit::{AppKit, apply, install_terminate_handler};

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// A window that records every operation a raise makes, so the assertion is
    /// on the calls rather than on a report the raise wrote for itself.
    #[derive(Debug, Default)]
    struct Recorded {
        minimised: bool,
        /// Whether `unminimise` refuses: the step is asked for and does not
        /// take, which is the case the report has to keep out.
        refuses_unminimise: bool,
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
            self.record(Step::Unminimise);
            !self.refuses_unminimise
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

    #[test]
    fn a_terminate_asks_the_apps_own_quit_flow_and_is_not_carried_out_by_appkit() {
        // `⌘Q` is not a second way out. AppKit's terminate makes the same request
        // the tray's `Quit` row makes, and is then refused, so the flow's own
        // close is what ends the process rather than AppKit's terminate.
        let asked = Cell::new(0);
        let may_terminate = answer_terminate(&|| asked.set(asked.get() + 1));
        assert_eq!(
            asked.get(),
            1,
            "AppKit's terminate asked for the app's own quit flow"
        );
        assert!(
            !may_terminate,
            "and AppKit did not carry it out: the flow's close ends the process"
        );
    }

    /// A window that refuses every operation: what a raise finds when the window
    /// it looked up has gone in between.
    #[derive(Debug, Default)]
    struct Gone;

    impl Window for Gone {
        fn minimised(&self) -> bool {
            false
        }

        fn order_front(&self) -> bool {
            false
        }

        fn make_key_and_main(&self) -> bool {
            false
        }

        fn unminimise(&self) -> bool {
            false
        }

        fn activate_app(&self) -> bool {
            false
        }
    }

    #[test]
    fn a_step_that_did_not_take_is_left_out_of_the_report() {
        // The report is a report, not a plan: every call answers whether it took,
        // and the answer is what the line says. A window that has gone reports
        // nothing, which is the AppKit case of a lookup that found no window,
        // and it does not claim to have brought anything forward.
        assert_eq!(settings_asked(true, &Gone), Some(Vec::new()));

        // A step that was asked for and refused is left out while the others go
        // through. The call is recorded, so this says why the step is missing:
        // it was asked for and did not take, rather than never having been tried
        // at all. That is the difference between "the window was brought
        // forward" and "the app tried to bring it forward".
        let stuck = Recorded {
            minimised: true,
            refuses_unminimise: true,
            ..Recorded::default()
        };
        assert_eq!(
            raise(&stuck),
            vec![Step::Front, Step::KeyAndMain, Step::Activate],
            "the unminimise was asked for and refused"
        );
        assert!(
            stuck.calls().contains(&Step::Unminimise),
            "it was asked for: {:?}",
            stuck.calls()
        );
    }
}
