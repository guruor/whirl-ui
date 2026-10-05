//! The About pane's facts: what this app is, which build is running, where its
//! source is, and the one request this app makes to the network.
//!
//! Three rules shape this module, and each is a value or a signature rather than
//! a claim:
//!
//! - **The version comes from the bundle, not from this file.** [`bundle_version`]
//!   reads `CFBundleShortVersionString` out of the `Info.plist` of the app bundle
//!   the running executable sits in; [`binary_version`] is the string the binary
//!   itself reports, and it is the fallback when there is no bundle. The pane
//!   shows both when they disagree, because a build installed over another is
//!   exactly that disagreement and it is a diagnostic.
//! - **Nothing is sent but the request.** [`check`] is one `GET` to this app's
//!   published releases, made with the system's own `curl`. There is no query
//!   string, no body, no cookie, no token and no header naming this app; `curl`
//!   sends its own default `User-Agent` because that is what `curl` does, and no
//!   identifier of ours rides along. `-q` makes the request ignore any
//!   `~/.curlrc`, so it carries nothing this app did not ask for.
//! - **A check that could not be made says so.** An unreachable network, an API
//!   that refused the request and a repository with no published release are
//!   three failures and never "up to date": [`Check::CouldNot`] carries the
//!   reason and the pane draws the reason it was given.
//!
//! Nothing here runs by itself. [`check`] is called from two places and both are
//! the same on-demand path: the About pane's button, and the `--check-update`
//! command line that drives that button's own call. There is no timer, thread or
//! interval in this crate that reaches it.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The repository this app's source is published at.
///
/// It is the manifest's own `repository` field rather than a second copy of the
/// URL in this file, so the pane and a report cannot name two different places.
pub const SOURCE_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// The version this binary reports, which is what the pane falls back to when it
/// is not inside an app bundle.
pub const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The prefix a GitHub repository URL carries, which the releases API is derived
/// from.
const GITHUB: &str = "https://github.com/";

/// The system's HTTP client, by absolute path so nothing here depends on the
/// caller's `PATH`.
const CURL: &str = "/usr/bin/curl";

/// How long the check waits for an answer before giving up.
const CURL_TIMEOUT: &str = "10";

/// The version the running bundle declares, when the executable is inside one.
///
/// `None` where there is no bundle, which is the ordinary state of `cargo run`
/// and of a binary copied out on its own; the pane then shows
/// [`binary_version`].
pub fn bundle_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    bundle_version_of(&exe)
}

/// [`bundle_version`] for a named executable, so a test can point it at a
/// bundle of its own.
pub fn bundle_version_of(exe: &Path) -> Option<String> {
    let plist = bundle_plist(exe)?;
    let text = std::fs::read_to_string(plist).ok()?;
    plist_string(&text, "CFBundleShortVersionString")
}

/// The version string this binary reports.
pub fn binary_version() -> &'static str {
    BINARY_VERSION
}

/// The version the app is running as: the bundle's when it is inside one, this
/// binary's when it is not.
pub fn running_version() -> String {
    bundle_version().unwrap_or_else(|| BINARY_VERSION.to_string())
}

/// The `Contents/Info.plist` of the bundle the executable `exe` sits in, when it
/// sits in one.
///
/// The path is `<anything>.app/Contents/MacOS/<binary>`; anything else is not a
/// bundle, and a `.app` whose plist is missing is treated the same way rather
/// than guessed at.
fn bundle_plist(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    if file_name_of(macos) != Some("MacOS") {
        return None;
    }
    let contents = macos.parent()?;
    if file_name_of(contents) != Some("Contents") {
        return None;
    }
    if contents.parent()?.extension().and_then(|ext| ext.to_str()) != Some("app") {
        return None;
    }
    let plist = contents.join("Info.plist");
    plist.is_file().then_some(plist)
}

/// One path component as a string, when it is one.
fn file_name_of(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

/// The value of `key` in an XML property list, as a string.
///
/// The one shape this reads is the one `Info.plist` is written in:
/// `<key>Name</key>` followed by `<string>value</string>`. A plist in another
/// form reads as absent, which puts the pane back on the binary's own version.
fn plist_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{key}</key>");
    let after = text.split_once(&needle)?.1.trim_start();
    let value = after
        .strip_prefix("<string>")?
        .split_once("</string>")?
        .0
        .trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// The latest published release, as the check reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The release's version, with any leading `v` removed.
    pub version: String,
    /// The release's page, which is what a reader opens.
    pub page: String,
}

impl Release {
    /// The release an API answer names, or why the answer is not one.
    fn from_body(body: &str) -> Result<Release, String> {
        let value: Value = serde_json::from_str(body)
            .map_err(|error| format!("GitHub's answer could not be read: {error}"))?;
        let tag = value
            .get("tag_name")
            .and_then(Value::as_str)
            .ok_or_else(|| "GitHub's answer named no release".to_string())?;
        let page = value
            .get("html_url")
            .and_then(Value::as_str)
            .ok_or_else(|| "GitHub's answer named no release page".to_string())?;
        Ok(Release {
            version: tag.trim().trim_start_matches('v').to_string(),
            page: page.trim().to_string(),
        })
    }
}

/// What the check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// A newer version than the one running is published.
    Newer(Release),
    /// The running version is the newest one published, and this is it.
    Newest(Release),
    /// The check could not be made, and this is why.
    CouldNot {
        /// The reason, in the words of whatever failed.
        reason: String,
    },
}

impl Check {
    /// The line the pane carries and the command line prints.
    pub fn line(&self) -> String {
        match self {
            Check::Newer(release) => format!(
                "a newer version is published: {}, at {}",
                release.version, release.page
            ),
            Check::Newest(release) => {
                format!("this is the newest version published, {}", release.version)
            }
            Check::CouldNot { reason } => format!("the check could not be made: {reason}"),
        }
    }
}

/// What `running` is against the published `release`.
///
/// The decision is a function of a value rather than prose inside the request,
/// so a test and a screenshot can put a release in and read the line out with no
/// network and with no release published upstream.
pub fn judge(running: &str, release: &Release) -> Check {
    if order(&release.version, running) == Ordering::Greater {
        Check::Newer(release.clone())
    } else {
        Check::Newest(release.clone())
    }
}

/// Run the check: one anonymous `GET` to this app's published releases.
///
/// The window's only caller is the About pane's button, and the command line's
/// `--check-update` mode drives the same call. Nothing schedules it.
pub fn check() -> Check {
    let Some(url) = releases_url() else {
        return Check::CouldNot {
            reason: format!("the build manifest names no GitHub repository: {SOURCE_URL}"),
        };
    };
    match latest_release(&url) {
        Ok(release) => judge(&running_version(), &release),
        Err(reason) => Check::CouldNot { reason },
    }
}

/// This app's releases API, when the manifest names a GitHub repository.
fn releases_url() -> Option<String> {
    let slug = SOURCE_URL.strip_prefix(GITHUB)?;
    Some(format!(
        "https://api.github.com/repos/{slug}/releases/latest"
    ))
}

/// One anonymous GET, and the latest release its answer names.
///
/// A transport failure is one of curl's own words, which is why the pane can say
/// what went wrong rather than only that something did. An HTTP answer is read
/// by its status: `404` is a repository with nothing published, `403` is the
/// anonymous rate limit, and anything else is reported as it came.
fn latest_release(url: &str) -> Result<Release, String> {
    let output = std::process::Command::new(CURL)
        .args([
            "-q",
            "-sS",
            "--max-time",
            CURL_TIMEOUT,
            "--proto",
            "=https",
            "-w",
            "\n%{http_code}",
            url,
        ])
        .output()
        .map_err(|error| format!("cannot run {CURL}: {error}"))?;
    if !output.status.success() {
        return Err(curl_reason(&output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, code) = text
        .rsplit_once('\n')
        .ok_or_else(|| "curl printed no HTTP status".to_string())?;
    match code.trim() {
        "200" => Release::from_body(body),
        "404" => Err(format!("no release is published for {SOURCE_URL} yet")),
        "403" => Err(
            "GitHub refused the request (HTTP 403): a check without a key is rate limited to 60 an \
             hour, and this machine is over it"
                .to_string(),
        ),
        other => Err(format!("GitHub answered HTTP {other}")),
    }
}

/// Why curl could not make the request, in curl's own words.
fn curl_reason(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr.trim();
    if !reason.is_empty() {
        return reason.to_string();
    }
    match output.status.code() {
        Some(code) => format!("curl exited {code}"),
        None => "curl was killed by a signal".to_string(),
    }
}

/// Numeric order of two version strings.
///
/// The three numbers of a dotted version are compared as numbers, and everything
/// after the third, or after the first non-digit, is ignored: `0.1.0-local`
/// reads as `0.1.0`, which is what a build between two releases should read as.
fn order(left: &str, right: &str) -> Ordering {
    triple(left).cmp(&triple(right))
}

/// The first three numbers of a dotted version, missing ones as `0`.
fn triple(text: &str) -> [u64; 3] {
    let mut parts = [0u64; 3];
    for (slot, part) in text.split('.').take(3).enumerate() {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        parts[slot] = digits.parse().unwrap_or(0);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three outcomes, with one line each, printed so a run can be read and
    /// quoted. This is the check's whole vocabulary: newer, newest, or a reason
    /// it could not be made.
    #[test]
    fn the_three_outcomes_are_one_line_each_and_never_a_false_uptodate() {
        let newer = judge(
            "0.1.0",
            &Release {
                version: "0.2.0".to_string(),
                page: "https://github.com/guruor/whirl-ui/releases/tag/v0.2.0".to_string(),
            },
        );
        let newest = judge(
            "0.1.0",
            &Release {
                version: "0.1.0".to_string(),
                page: "https://github.com/guruor/whirl-ui/releases/tag/v0.1.0".to_string(),
            },
        );
        let failed = Check::CouldNot {
            reason: "Could not resolve host: api.github.com".to_string(),
        };
        let lines: Vec<String> = [&newer, &newest, &failed]
            .into_iter()
            .map(Check::line)
            .collect();
        for line in &lines {
            eprintln!("{line}");
        }
        assert_eq!(
            lines[0],
            "a newer version is published: 0.2.0, at \
             https://github.com/guruor/whirl-ui/releases/tag/v0.2.0"
        );
        assert_eq!(lines[1], "this is the newest version published, 0.1.0");
        // The failure path names itself as a failure, and never reads as the
        // newest: that is the one outcome that gets faked everywhere.
        assert_eq!(
            lines[2],
            "the check could not be made: Could not resolve host: api.github.com"
        );
        assert!(lines[2].contains("could not be made"), "{}", lines[2]);
        assert!(!lines[2].contains("newest"), "{}", lines[2]);
    }

    /// A release the running build is ahead of is the newest, not "newer": a
    /// checkout between two releases must not be told to update to an older one.
    #[test]
    fn a_running_version_ahead_of_the_release_reads_as_the_newest() {
        let release = Release {
            version: "0.1.0".to_string(),
            page: "https://github.com/guruor/whirl-ui/releases/tag/v0.1.0".to_string(),
        };
        assert!(matches!(judge("0.2.0", &release), Check::Newest(_)));
        assert!(matches!(judge("0.1.1", &release), Check::Newest(_)));
        assert!(matches!(judge("0.1.0-local", &release), Check::Newest(_)));
        assert!(matches!(judge("0.0.9", &release), Check::Newer(_)));
    }

    #[test]
    fn a_version_is_compared_by_its_numbers_and_not_as_text() {
        assert_eq!(order("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(order("1.0.0", "0.99.99"), Ordering::Greater);
        assert_eq!(order("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(triple("1.2.3-rc1"), [1, 2, 3]);
        assert_eq!(triple("1.2"), [1, 2, 0]);
    }

    #[test]
    fn the_releases_answer_is_read_for_its_tag_and_its_page() {
        let body = r#"{
            "tag_name": "v0.2.0",
            "name": "whirl-ui 0.2.0",
            "html_url": "https://github.com/guruor/whirl-ui/releases/tag/v0.2.0",
            "draft": false
        }"#;
        let release = Release::from_body(body).expect("a release");
        assert_eq!(release.version, "0.2.0");
        assert_eq!(
            release.page,
            "https://github.com/guruor/whirl-ui/releases/tag/v0.2.0"
        );
        // An answer with no release in it is a reason, not a version.
        assert!(Release::from_body("{\"message\":\"Not Found\"}").is_err());
        assert!(Release::from_body("not json").is_err());
    }

    #[test]
    fn an_info_plist_is_read_for_the_bundle_version() {
        let text = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Whirl</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.1</string>
</dict>
</plist>
"#;
        assert_eq!(
            plist_string(text, "CFBundleShortVersionString"),
            Some("0.1.1".to_string())
        );
        assert_eq!(plist_string(text, "CFBundleVersion"), None);
    }

    /// A bundle laid out the way macOS lays one out, so the path rule is held
    /// with no installed app: `<Foo>.app/Contents/MacOS/<binary>`.
    #[test]
    fn the_version_comes_from_the_bundle_the_executable_sits_in() {
        let root = std::env::temp_dir().join(format!("whirlui-bundle-{}", std::process::id()));
        let macos = root.join("Whirl.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).expect("a bundle");
        std::fs::write(
            root.join("Whirl.app/Contents/Info.plist"),
            "<plist><dict><key>CFBundleShortVersionString</key><string>9.9.9</string></dict></plist>",
        )
        .expect("a plist");

        let binary = macos.join("whirl-ui");
        std::fs::write(&binary, "").expect("a binary");
        assert_eq!(bundle_version_of(&binary), Some("9.9.9".to_string()));

        // A binary outside a bundle has none, and the pane falls back.
        let loose = root.join("whirl-ui");
        std::fs::write(&loose, "").expect("a loose binary");
        assert_eq!(bundle_version_of(&loose), None);
        assert_eq!(binary_version(), BINARY_VERSION);
    }

    /// The releases URL is derived from the manifest's repository, so the pane's
    /// source link and the check cannot point at two different repositories.
    #[test]
    fn the_check_asks_the_repository_the_pane_names() {
        let slug = SOURCE_URL
            .strip_prefix(GITHUB)
            .expect("the manifest names a GitHub repository");
        let url = releases_url().expect("a GitHub repository");
        // The API is derived from the same manifest field the pane prints, so a
        // reader checking the source link is looking at what the check asks.
        assert_eq!(
            url,
            format!("https://api.github.com/repos/{slug}/releases/latest")
        );
        assert!(url.starts_with("https://api.github.com/repos/"), "{url}");
        assert!(url.contains(slug), "{url}");
        assert_eq!(SOURCE_URL, "https://github.com/guruor/whirl-ui");
    }

    /// One live read of a repository that has releases, so the request path is
    /// exercised end to end and not only the parser. Ignored by default because
    /// it needs a network, which the suite must not: run it with
    /// `cargo test -p whirl-ui -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a network; run it explicitly for evidence"]
    fn the_live_request_reads_a_real_published_release() {
        let url = "https://api.github.com/repos/rust-lang/rust/releases/latest";
        let release = latest_release(url).expect("a published release");
        eprintln!("live: {} at {}", release.version, release.page);
        assert!(!release.version.is_empty());
        assert!(release.page.starts_with("https://github.com/"));
        // Running an old build against it is "newer"; running its own version is
        // "the newest".
        assert!(matches!(judge("0.1.0", &release), Check::Newer(_)));
        let same = judge(&release.version, &release);
        assert!(matches!(same, Check::Newest(_)));
        eprintln!("newer: {}", judge("0.1.0", &release).line());
        eprintln!("newest: {}", same.line());
    }
}
