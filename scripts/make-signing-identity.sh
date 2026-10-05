#!/bin/sh
#
# The app's local signing identity: one self-signed code-signing certificate,
# created once in the login keychain, that every local build is signed with.
#
#   usage: scripts/make-signing-identity.sh [create|status|delete]
#
# Why it exists. Whirl.app has no Developer ID and is not notarized, so it is
# signed locally. Signed ad-hoc (`codesign --sign -`), the app's *designated
# requirement* is the hash of its own binary:
#
#   $ codesign -d -r- dist/Whirl.app
#   designated => cdhash H"fdbf5c9749a221e15528ceebe70f697725f73528"
#
# The designated requirement is the identity macOS remembers a launch approval
# against, so a rebuild is a different app as far as macOS is concerned and the
# approval does not carry over: the operator is asked again, every time. Signed
# with this certificate instead, the requirement is the bundle identifier plus
# the certificate, and neither moves when the binary does:
#
#   designated => identifier "com.guruor.whirl-ui" and certificate leaf = H"..."
#
# The certificate is self-signed, so it is not trusted and `find-identity -v`
# reports no valid identity. codesign signs with it anyway, which was measured
# on macOS 26.7 rather than assumed: what codesign needs is the identity to be
# in a keychain on the search list, and the login keychain is on it. The
# certificate says nothing about who built the app and it does not make `spctl`
# pass -- `spctl -a -vv` goes on refusing the bundle, because a self-signed app
# is not notarized, and this script does not pretend otherwise. What it buys is
# a stable identity; the row that matters is the number of approvals macOS asks
# for, not the verdict.
#
# Nothing here needs sudo, a password, a network or an answer. The private key
# is generated here, goes into the login keychain, and is never written into
# this repository or printed anywhere.
#
# Remove it again with either of these, both of which need nothing else:
#
#   scripts/make-signing-identity.sh delete
#   security delete-identity -c "Whirl Local Signing" ~/Library/Keychains/login.keychain-db
#
# `security delete-certificate -c "Whirl Local Signing" ~/Library/Keychains/login.keychain-db`
# removes the certificate too, but leaves the private key behind in the
# keychain, which is why this script uses delete-identity instead.

set -eu

name="Whirl Local Signing"

# The keychain the identity lives in: the login keychain, because it is on the
# keychain search list, and codesign only finds an identity in a keychain it
# searches (`--keychain <path>` for a keychain off the list does not work --
# measured on macOS 26.7). WHIRL_SIGNING_KEYCHAIN overrides it, and it is there so
# the create/status/delete cycle can be exercised against a throwaway keychain
# without touching the operator's own. An identity in a keychain that is not on
# the search list is created and deleted all the same; codesign just cannot use it.
keychain=${WHIRL_SIGNING_KEYCHAIN:-$(security default-keychain -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')}

# have: is the identity already in the keychain? The policy is code signing, so
# a certificate created for something else with the same name does not count.
have() {
    security find-identity -p codesigning "$keychain" 2>/dev/null | grep -q "\"$name\""
}

usage() {
    cat <<'USAGE'
scripts/make-signing-identity.sh [create|status|delete]

  create   create the certificate in the login keychain if it is not there (default)
  status   say whether builds are signed with it or ad-hoc
  delete   remove the certificate and its private key again

The certificate is self-signed, needs no sudo and is not a Developer ID. See the
top of this script for why it exists and what it does not fix.

Set WHIRL_SIGNING_KEYCHAIN=<path> to work on that keychain instead of the login
keychain. It is for a throwaway keychain in a test; an identity in a keychain off
the search list is created and deleted all the same, but codesign cannot sign
with it.
USAGE
}

case "${1-create}" in
create)
    if have; then
        echo "signing-identity: $name is already in $keychain, so nothing was created"
        exit 0
    fi
    for tool in openssl security; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo "signing-identity: $tool is not on PATH; this script needs openssl and security" >&2
            exit 1
        fi
    done

    work=$(mktemp -d "${TMPDIR:-/tmp}/whirl-signing.XXXXXX")
    trap 'rm -rf "$work"' EXIT HUP INT TERM

    # The three extensions are what make this a code-signing certificate rather
    # than a key: digital signature use, code-signing purpose, and not a CA.
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
        -keyout "$work/key.pem" -out "$work/cert.pem" \
        -subj "/CN=$name" \
        -addext 'basicConstraints=critical,CA:false' \
        -addext 'keyUsage=critical,digitalSignature' \
        -addext 'extendedKeyUsage=critical,codeSigning' 2>/dev/null

    # The key and the certificate go into the keychain as PEM, not as a PKCS#12
    # archive. A PKCS#12 needs a passphrase, and the passphrase is one more
    # thing to generate, pass around and get wrong -- `security import` reports
    # a wrong one as "MAC verification failed during PKCS12 import", which was
    # seen once here. PEM needs no passphrase at all, so there is nothing to get
    # wrong. `-f pemseq` because the file holds two items.
    cat "$work/key.pem" "$work/cert.pem" > "$work/identity.pem"

    # -T, not -A: codesign is allowed to use the key without the keychain
    # warning it would otherwise put up, and no other application is. Without
    # one of the two flags macOS prompts on every build, which is the thing this
    # script exists to remove. The keychain is not unlocked here: the login
    # keychain is already unlocked while its owner is logged in.
    security import "$work/identity.pem" -k "$keychain" -f pemseq \
        -T /usr/bin/codesign -T /usr/bin/security >/dev/null

    if ! have; then
        echo "signing-identity: the certificate was imported but codesign cannot see it in $keychain" >&2
        exit 1
    fi
    echo "signing-identity: created $name in $keychain"
    echo "signing-identity: scripts/make-bundle.sh signs with it from now on; it is not notarized,"
    echo "signing-identity: so spctl -a -vv will go on refusing the bundle, which is expected"
    ;;
status)
    if have; then
        security find-identity -p codesigning "$keychain" | grep "$name"
        echo "signing-identity: $name is in $keychain; builds are signed with it"
    else
        echo "signing-identity: $name is not in $keychain; builds are signed ad-hoc"
        echo "signing-identity: run 'scripts/make-signing-identity.sh' once to create it"
    fi
    ;;
delete)
    if ! have; then
        echo "signing-identity: $name is not in $keychain, so there is nothing to delete"
        exit 0
    fi
    security delete-identity -c "$name" "$keychain"
    echo "signing-identity: deleted $name and its private key from $keychain"
    ;;
-h | --help)
    usage
    ;;
*)
    usage >&2
    exit 2
    ;;
esac
