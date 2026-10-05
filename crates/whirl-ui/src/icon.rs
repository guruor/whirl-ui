//! The menu bar item's artwork.
//!
//! The tray icon is the one part of the menu bar a screenshot proves and a test
//! cannot: AppKit draws it next to the clock. So the two things that can be
//! wrong about it are values here rather than calls in `tray`:
//!
//! - **which mark a state calls for**, and
//! - **what a mark is made of**, which for a *template* image is the whole
//!   point.
//!
//! macOS draws a template image from its alpha channel alone, which is what
//! lets it invert the artwork for a light and a dark menu bar. So every pixel
//! the artwork inks is black and the shape is the alpha, and the tests below
//! hold that: a grey or a colour in the file would be correct in one appearance
//! and wrong in the other, which is a mistake that a screenshot at one
//! appearance cannot catch.
//!
//! The PNGs are decoded at runtime rather than converted to raw bytes at build
//! time. `png` is already a dependency (the `--screenshot` writer), the six
//! files together are about 3 kB, and a file that stops decoding is then a
//! message on stderr rather than a compile error in a module nobody edits.
//!
//! Nothing here touches `tray-icon`: the crate is a macOS-only dependency, and
//! this module is reached by the three-platform test run, which is where the
//! artwork is checked.

// The tray is macOS-only for now (docs/milestones.md M4), so on the other two
// legs what only it calls is reached by the tests and by nothing else.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::error::Error;
use std::fmt;
use std::io::Cursor;

use crate::state::View;

/// The running mark: the whirl, at two pixels per point.
const RUNNING_2X: &[u8] = include_bytes!("../assets/tray-iconTemplate@2x.png");
/// The paused mark: the whirl's outer turn wound around a pause, at two pixels
/// per point.
const PAUSED_2X: &[u8] = include_bytes!("../assets/tray-icon-pausedTemplate@2x.png");
/// The unreachable mark: the whirl's outer turn with a break in it, at two
/// pixels per point.
const UNREACHABLE_2X: &[u8] = include_bytes!("../assets/tray-icon-unreachableTemplate@2x.png");

// The 1x files ship beside the 2x ones and the tests below are what checks them;
// the tray sets the 2x file only (`Mark::png`), so the binary does not carry
// these three. A 1x file that stopped decoding still fails the suite, which runs
// on all three platforms.
#[cfg(test)]
const RUNNING_1X: &[u8] = include_bytes!("../assets/tray-iconTemplate.png");
#[cfg(test)]
const PAUSED_1X: &[u8] = include_bytes!("../assets/tray-icon-pausedTemplate.png");
#[cfg(test)]
const UNREACHABLE_1X: &[u8] = include_bytes!("../assets/tray-icon-unreachableTemplate.png");

/// Which of the marks the menu bar item is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// The daemon answered and the schedule is live: the whirl.
    Running,
    /// The daemon answered and reports `paused=true`: the whirl paused.
    Paused,
    /// The daemon did not answer: the whirl's outer turn, broken.
    Unreachable,
}

impl Mark {
    /// The mark the daemon's state calls for.
    ///
    /// Three states, and one source of truth for each. The app reaches the
    /// daemon or it does not, and that is the same connection outcome the
    /// window draws in its own words (`Daemon::NotRunning`, "whirl is not
    /// running"); this is not a second opinion about it, it is the same fact
    /// rendered as a mark rather than a sentence. A reached daemon reports
    /// `paused` in its `status` (2.10) or it does not.
    ///
    /// An unreachable daemon is not a paused one: `paused` is a value the
    /// daemon's `status` carries, never something to infer from silence, so
    /// silence is its own mark rather than a pause the app guessed. It is not a
    /// running one either: with no `status` there is nothing to say the
    /// schedule is live.
    pub fn of(view: &View) -> Mark {
        if !view.reachable() {
            Mark::Unreachable
        } else if view.paused() {
            Mark::Paused
        } else {
            Mark::Running
        }
    }

    /// The file this mark ships as, which is the one the tray sets.
    ///
    /// The 2x one. `tray-icon` builds one `NSImage` from one raster and caps
    /// what it hands AppKit at 22 pt (`MAX_ICON_HEIGHT` in its macOS backend),
    /// so on a 2x display the menu bar scales from the pixels it was given:
    /// twice the pixels is the better source for that, and the 1x file is the
    /// one that would have to be scaled up by two. Both files ship because both
    /// are what a template image is carried as.
    fn png(self) -> &'static [u8] {
        match self {
            Mark::Running => RUNNING_2X,
            Mark::Paused => PAUSED_2X,
            Mark::Unreachable => UNREACHABLE_2X,
        }
    }

    /// The mark, decoded, as the tray takes it.
    pub fn artwork(self) -> Result<Artwork, Unreadable> {
        Artwork::decode(self.png())
    }
}

/// One decoded mark: black pixels, and the alpha channel that shapes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    /// Straight (not premultiplied) RGBA, eight bits per channel.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Artwork {
    /// Decode one of the shipped PNGs.
    ///
    /// Only the shape the artwork is carried in is accepted: 8-bit RGBA. The
    /// decoder is asked for no transformations, so a file that somehow stopped
    /// being RGBA is refused here rather than quietly converted into a colour
    /// this module never meant to ship.
    pub fn decode(bytes: &[u8]) -> Result<Artwork, Unreadable> {
        let mut reader = png::Decoder::new(Cursor::new(bytes))
            .read_info()
            .map_err(Unreadable::Png)?;
        let mut rgba = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut rgba).map_err(Unreadable::Png)?;
        if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
            return Err(Unreadable::Shape {
                color: frame.color_type,
                depth: frame.bit_depth,
            });
        }
        rgba.truncate(frame.buffer_size());
        Ok(Artwork {
            rgba,
            width: frame.width,
            height: frame.height,
        })
    }
}

/// Why a shipped mark could not be read.
#[derive(Debug)]
pub enum Unreadable {
    /// The file is not a PNG this decoder understands.
    Png(png::DecodingError),
    /// The file is a PNG of a shape the artwork is not carried in.
    Shape {
        color: png::ColorType,
        depth: png::BitDepth,
    },
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unreadable::Png(error) => write!(f, "the mark is not a readable PNG: {error}"),
            Unreadable::Shape { color, depth } => write!(
                f,
                "the mark is {depth:?} {color:?}, and the artwork is 8-bit RGBA"
            ),
        }
    }
}

impl Error for Unreadable {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Unreadable::Png(error) => Some(error),
            Unreadable::Shape { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whirlui_client::{Event, Status};

    /// Every file this module ships: what it is, its bytes, and its side in
    /// pixels.
    const SHIPPED: [(&str, &[u8], u32); 6] = [
        ("running 1x", RUNNING_1X, 16),
        ("running 2x", RUNNING_2X, 32),
        ("paused 1x", PAUSED_1X, 16),
        ("paused 2x", PAUSED_2X, 32),
        ("unreachable 1x", UNREACHABLE_1X, 16),
        ("unreachable 2x", UNREACHABLE_2X, 32),
    ];

    /// A `status` body, as the daemon prints the keys this module reads.
    fn status(paused: bool) -> Status {
        let lines: Vec<String> = [
            "daemon_version: whirl 0.1.0".to_string(),
            "protocol: 2".to_string(),
            "seq: 3".to_string(),
            format!("paused: {}", u8::from(paused)),
            "last_digest: d43584".to_string(),
            "last_origin_key: pictures:99".to_string(),
        ]
        .to_vec();
        Status::from_lines(&lines)
    }

    fn pixels(art: &Artwork) -> impl Iterator<Item = [u8; 4]> + '_ {
        art.rgba
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
    }

    #[test]
    fn every_shipped_mark_decodes_at_the_size_it_is_named_for() {
        for (what, bytes, side) in SHIPPED {
            let art = Artwork::decode(bytes).unwrap_or_else(|error| panic!("{what}: {error}"));
            assert_eq!((art.width, art.height), (side, side), "{what}");
            assert_eq!(art.rgba.len(), (side * side * 4) as usize, "{what}");
        }
    }

    #[test]
    fn a_template_image_is_black_plus_alpha_and_nothing_else() {
        // The required change this guards is a mark that bakes in a colour or a
        // grey: macOS draws a template image from the alpha alone, so any RGB in
        // the file is either dead weight or, worse, the reason the artwork looks
        // wrong in one of the two menu bar appearances.
        for (what, bytes, _) in SHIPPED {
            let art = Artwork::decode(bytes).unwrap_or_else(|error| panic!("{what}: {error}"));
            for pixel in pixels(&art) {
                assert_eq!(
                    [pixel[0], pixel[1], pixel[2]],
                    [0, 0, 0],
                    "{what} has a coloured pixel"
                );
            }
        }
    }

    #[test]
    fn a_mark_is_a_shape_rather_than_a_block() {
        // The other way a template image goes wrong is being drawn edge to edge,
        // which is a filled rectangle in the menu bar and not a mark.
        for (what, bytes, _) in SHIPPED {
            let art = Artwork::decode(bytes).unwrap_or_else(|error| panic!("{what}: {error}"));
            let inked = pixels(&art).filter(|pixel| pixel[3] > 0).count();
            let clear = pixels(&art).filter(|pixel| pixel[3] == 0).count();
            assert!(inked > 0, "{what} draws nothing");
            assert!(clear > 0, "{what} is opaque everywhere");
            assert!(
                inked < (art.width * art.height) as usize,
                "{what} fills its own canvas"
            );
        }
    }

    #[test]
    fn the_two_scales_of_a_mark_are_the_same_picture() {
        // Blowing the 1x file up by two should land on the 2x file's ink: the
        // two are one drawing, not two that drifted apart.
        for (one, two) in [
            (RUNNING_1X, RUNNING_2X),
            (PAUSED_1X, PAUSED_2X),
            (UNREACHABLE_1X, UNREACHABLE_2X),
        ] {
            let small = Artwork::decode(one).expect("1x");
            let large = Artwork::decode(two).expect("2x");
            let inked = |art: &Artwork| {
                pixels(art)
                    .map(|pixel| u32::from(pixel[3] > 128))
                    .sum::<u32>()
            };
            let small_inked = inked(&small) * 4;
            let large_inked = inked(&large);
            let ratio = f64::from(small_inked) / f64::from(large_inked);
            assert!(
                (0.7..1.3).contains(&ratio),
                "the scales disagree: {small_inked} inked pixels at 1x, {large_inked} at 2x"
            );
        }
    }

    #[test]
    fn the_three_marks_are_three_pictures() {
        // A state indicator that two states share is not an indicator: every
        // pair of marks has to be a different drawing, at both scales.
        let at1x = [RUNNING_1X, PAUSED_1X, UNREACHABLE_1X];
        let at2x = [RUNNING_2X, PAUSED_2X, UNREACHABLE_2X];
        for files in [at1x, at2x] {
            for (i, left) in files.iter().enumerate() {
                for right in &files[i + 1..] {
                    assert_ne!(left, right, "two marks are the same file");
                }
            }
        }
    }

    /// The box a mark's ink lands in, in pixels of that mark's own canvas:
    /// left, top, right, bottom, inclusive. Any ink counts, because any ink is
    /// drawn: the shape is the alpha.
    fn ink_box(art: &Artwork) -> (u32, u32, u32, u32) {
        let mut boxed = (art.width, art.height, 0, 0);
        for (i, pixel) in art.rgba.chunks_exact(4).enumerate() {
            if pixel[3] == 0 {
                continue;
            }
            let (x, y) = (i as u32 % art.width, i as u32 / art.width);
            boxed = (
                boxed.0.min(x),
                boxed.1.min(y),
                boxed.2.max(x),
                boxed.3.max(y),
            );
        }
        boxed
    }

    #[test]
    fn the_three_marks_ink_the_same_box() {
        // A state change may not resize the item. The item is as wide as its
        // artwork (the tray sets a mark and no title), so the marks have to
        // land on the same pixels of the same canvas: a mark drawn in a wider
        // or a narrower part of its own box would move the item's edges
        // whenever the state changed. This is what "one drawing in the same
        // 16x16 box" means for all three, checked rather than assumed.
        for files in [
            [RUNNING_1X, PAUSED_1X, UNREACHABLE_1X],
            [RUNNING_2X, PAUSED_2X, UNREACHABLE_2X],
        ] {
            let boxes: Vec<(u32, u32, u32, u32)> = files
                .iter()
                .map(|bytes| ink_box(&Artwork::decode(bytes).expect("a mark")))
                .collect();
            assert_eq!(
                boxes[0], boxes[1],
                "the running and paused marks ink different boxes: {:?} and {:?}",
                boxes[0], boxes[1]
            );
            assert_eq!(
                boxes[0], boxes[2],
                "the unreachable mark inks a different box: {:?} and {:?}",
                boxes[0], boxes[2]
            );
        }
    }

    #[test]
    fn the_pause_is_two_bars_at_sixteen_points() {
        // The item draws the mark at 16 points, so that is where the drawing is
        // judged, not at the size the SVG happens to be viewed at. At 16 pixels
        // a ring with its middle filled in is one blob: the pause has to stand
        // there as two runs of ink with a clear column between them, or the
        // state is not readable where it is actually read.
        //
        // The middle row crosses the ring at the canvas edges and the pause in
        // between, so the middle half of the row is the pause and nothing else.
        for (what, bytes) in [("paused 1x", PAUSED_1X), ("paused 2x", PAUSED_2X)] {
            let art = Artwork::decode(bytes).unwrap_or_else(|error| panic!("{what}: {error}"));
            let row = art.height / 2;
            let middle: Vec<u8> = (art.width / 4..art.width * 3 / 4)
                .map(|x| art.rgba[((row * art.width + x) * 4 + 3) as usize])
                .collect();

            let mut runs: Vec<Vec<usize>> = Vec::new();
            for (x, alpha) in middle.iter().enumerate() {
                if *alpha >= 128 {
                    match runs.last_mut() {
                        Some(run) if run.last() == Some(&(x - 1)) => run.push(x),
                        _ => runs.push(vec![x]),
                    }
                }
            }

            assert_eq!(
                runs.len(),
                2,
                "{what}: the middle row is not two bars: {runs:?}"
            );
            for run in &runs {
                assert!(run.len() >= 2, "{what}: a bar is one pixel wide: {runs:?}");
            }
            let gap = runs[1][0] - runs[0][runs[0].len() - 1] - 1;
            assert!(gap >= 1, "{what}: the two bars touch: {runs:?}");
        }
    }

    #[test]
    fn the_mark_follows_the_connection_and_then_the_pause() {
        assert_eq!(Mark::of(&View::live(status(false))), Mark::Running);
        assert_eq!(Mark::of(&View::live(status(true))), Mark::Paused);

        let mut view = View::live(status(false));
        view.apply(&Event::Paused);
        assert_eq!(Mark::of(&view), Mark::Paused);
        view.apply(&Event::Resumed);
        assert_eq!(Mark::of(&view), Mark::Running);
    }

    #[test]
    fn an_unreachable_daemon_is_its_own_mark_and_never_a_pause() {
        // The required change: the third mark, chosen from the connection
        // outcome the app already holds. A daemon this app cannot reach is not
        // running (the bug this fixes) and not paused either: there is no
        // `paused` value to read, so silence may not be read as one. The
        // assertion that it is not `Paused` is the point: if `Mark::of` folded
        // unreachable into a pause, the two states the daemon can actually
        // report would be indistinguishable from the one it cannot.
        let offline = View::offline();
        assert_eq!(Mark::of(&offline), Mark::Unreachable);
        assert!(!offline.paused(), "an unreachable daemon is not paused");
        assert_ne!(
            Mark::of(&offline),
            Mark::Paused,
            "an unreachable daemon must not be drawn as paused"
        );
        assert_ne!(
            Mark::of(&offline),
            Mark::Running,
            "an unreachable daemon must not be drawn as running"
        );
    }
}
