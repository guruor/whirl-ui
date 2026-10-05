#!/bin/sh
#
# Whirl.app: the release bundle, and the archive a release carries.
#
#   usage: scripts/make-bundle.sh [version]
#
# Writes, under dist/ (which is build output and not committed):
#
#   Whirl.app                    the app, an agent app with no Dock icon, signed locally
#   Whirl-<version>.zip          the archive, with Whirl.app at its root
#   Whirl-<version>.zip.sha256   the checksum published beside the archive
#
# The version comes from the tag. The release job passes the tag that triggered it,
# and a hand run with no argument uses the v* tag HEAD is exactly on. With neither,
# the bundle says it is a local build instead of borrowing a number it did not come
# from, because an artifact whose version is a guess is worse than one that admits
# what it is.
#
# The signature is local: no Developer ID and no notarization, which is a
# deliberate choice and not an oversight. `scripts/make-signing-identity.sh`
# creates a self-signed certificate in the login keychain once, and this script
# signs with it, so successive builds carry the same signing authority and the
# identity macOS remembers a launch approval against does not change on every
# rebuild. Where no such identity exists -- a release runner, a fresh checkout --
# the signature is ad-hoc, and the script says which of the two it used rather
# than leaving it to be discovered from `codesign -dv`. Either way a copy that
# was downloaded is quarantined and its first launch needs the documented step,
# and `spctl -a -vv Whirl.app` refuses it: an ad-hoc or self-signed app is not
# notarized, and that is expected rather than a failure.
#
# Nothing here needs sudo, a password, an interactive authorization or a network.
# codesign, ditto and shasum all ship with macOS; cargo is the pinned toolchain.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

# ---- the version -----------------------------------------------------------

# The version is the tag, and a tag is written with its `v`: the release job passes
# `v0.1.0` (that is what `github.ref_name` is), and the bundle's
# `CFBundleShortVersionString` is `0.1.0`. So the leading `v` comes off either way
# in, and a bare `0.1.0` is accepted beside it.
version=${1-}
version=${version#v}
if [ -z "$version" ]; then
    version=$(git describe --tags --exact-match --match 'v[0-9]*' 2>/dev/null || true)
    version=${version#v}
fi

local_build=no
if [ -z "$version" ]; then
    version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)
    version="$version-local"
    local_build=yes
fi

# ---- the tools -------------------------------------------------------------

for tool in cargo codesign ditto shasum; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "make-bundle: $tool is not on PATH; this script needs cargo (rustup), codesign, ditto and shasum" >&2
        exit 1
    fi
done

# The toolchain is the one rust-toolchain.toml pins. A version manager that puts
# its own shims ahead of rustup bypasses that pin (README, "Build and run"), so
# rustup's own cargo goes first when it is there.
if [ -x "$HOME/.cargo/bin/cargo" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
fi

# ---- the bundle ------------------------------------------------------------

# --locked: a release is built from the versions in Cargo.lock. Without it a
# release could quietly move a dependency, and the artifact would not be the tree
# the tag names.
cargo build --release --locked -p whirl-ui

app=dist/Whirl.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp target/release/whirl-ui "$app/Contents/MacOS/whirl-ui"
cp crates/whirl-ui/assets/app-icon.icns "$app/Contents/Resources/AppIcon.icns"

# The bundle's own description. Each key here is load-bearing:
#
#   LSUIElement            true: the app launches as an agent. No Dock tile and
#                          nothing in the switcher while its window is shut; its
#                          status item is all there is (M1 criterion 5, stated by
#                          the bundle as well as by the activation policy the app
#                          is built with). It is not the app's policy for its
#                          whole life: while the settings window is on screen the
#                          app moves itself to the regular policy, so the window
#                          is managed by the window manager and the app can be
#                          switched to, and it moves back when the window closes
#                          (`crates/whirl-ui/src/window.rs`).
#   CFBundleName           Whirl: the name macOS displays wherever the app is
#                          named, the Login Items pane included. This is the key
#                          that fixes the unrecognisable row the operator saw.
#   CFBundleIdentifier     the identity the login item is registered under, and
#                          the one `--login-item status` prints.
#   CFBundleShortVersionString / CFBundleVersion
#                          the tag. Change the tag and both move with it.
#   LSMinimumSystemVersion 13.0: `SMAppService.mainApp`, which is how the app
#                          registers its own login item, exists from macOS 13.
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Whirl</string>
    <key>CFBundleDisplayName</key>
    <string>Whirl</string>
    <key>CFBundleIdentifier</key>
    <string>com.guruor.whirl-ui</string>
    <key>CFBundleExecutable</key>
    <string>whirl-ui</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

# The signature. The identity is the one `scripts/make-signing-identity.sh`
# creates in the login keychain. That keychain is on the keychain search list,
# and that is what lets codesign see the identity: `--keychain <path>` for a
# keychain that is not on the list does not work (measured on macOS 26.7), which
# is why the identity lives in the login keychain rather than one of its own.
# The identifier is not passed here; codesign reads it from the Info.plist
# above, which is what the login item is registered under. `-` is the fallback
# for a machine with no identity on it, and the two are told apart in the output
# below.
signature=-
signed_with="an ad-hoc signature"
if command -v security >/dev/null 2>&1; then
    keychain=$(security default-keychain -d user 2>/dev/null | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')
    if [ -n "$keychain" ] &&
        security find-identity -p codesigning "$keychain" 2>/dev/null | grep -q '"Whirl Local Signing"'; then
        signature="Whirl Local Signing"
        signed_with="the local signing identity ($keychain)"
    fi
fi
codesign --force --sign "$signature" "$app"
codesign --verify --strict "$app"

# ---- the archive -----------------------------------------------------------

archive="dist/Whirl-$version.zip"
rm -f "$archive" "$archive.sha256"
# ditto, not zip: it keeps the bundle's structure and the extended attributes the
# signature is stored in.
ditto -c -k --sequesterRsrc --keepParent "$app" "$archive"
# The checksum file is written from inside dist/, so the name in it is the
# archive's own name and `shasum -a 256 -c Whirl-<version>.zip.sha256` finds it.
(cd dist && shasum -a 256 "Whirl-$version.zip" > "Whirl-$version.zip.sha256")

# ---- what was written ------------------------------------------------------

if [ "$local_build" = yes ]; then
    echo "make-bundle: HEAD is not on a v* tag and no version was given, so this build says it is $version"
fi
echo "make-bundle: $app"
echo "make-bundle: $app/Contents/MacOS/whirl-ui is the app; it is an agent app (LSUIElement) and it is signed with $signed_with"
echo "make-bundle: $archive"
cat "$archive.sha256"
echo "make-bundle: spctl -a -vv refuses this bundle, because a self-signed or ad-hoc app is not notarized"
echo "make-bundle: a downloaded copy of the archive is quarantined; its first launch needs the documented step"
