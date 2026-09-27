#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/macos-release-signing.sh
source "$SCRIPT_DIR/../macos-release-signing.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail=0
pass() { printf '[PASS] %s\n' "$1"; }
fail_test() {
    printf '[FAIL] %s\n' "$1" >&2
    fail=1
}

uname() { printf '%s\n' "${TEST_UNAME:-Darwin}"; }

unset VTCODE_MACOS_SIGNING_IDENTITY VTCODE_MACOS_NOTARY_PROFILE
if macos_release_signing_preflight >/dev/null 2>&1; then
    fail_test 'preflight must reject missing signing credentials'
else
    pass 'preflight rejects missing signing credentials'
fi

security() {
    if [[ "${TEST_IDENTITY_AVAILABLE:-1}" == "1" ]]; then
        printf '  1) ABCDEF "Developer ID Application: Test Developer (TEAMID)"\n'
        printf '     1 valid identities found\n'
    else
        printf '     0 valid identities found\n'
    fi
}

codesign() {
    printf '%s\n' "$@" >>"$tmp/codesign-args"
    return "${TEST_CODESIGN_STATUS:-0}"
}

ditto() {
    if [[ "$1" != "-c" || "$2" != "-k" || "$3" != "--norsrc" \
        || "$4" != "--noextattr" || "$5" != "--noqtn" ]]; then
        return 2
    fi
    cp "$6" "$7"
}

xcrun() {
    if [[ "$1" == "notarytool" && "$2" == "history" ]]; then
        return "${TEST_NOTARY_HISTORY_STATUS:-0}"
    fi
    if [[ "$1" == "notarytool" && "$2" == "submit" ]]; then
        printf '{"status":"%s"}\n' "${TEST_NOTARY_STATUS:-Accepted}"
        return 0
    fi
    return 2
}

spctl() {
    local assessed_binary
    for assessed_binary in "$@"; do :; done
    if [[ ! -x "$assessed_binary" ]]; then
        printf 'vtcode: executable bit is missing\n'
        return 1
    fi
    if [[ "${TEST_SPCTL_STATUS:-0}" != "0" ]]; then
        printf 'vtcode: rejected\n'
        return "$TEST_SPCTL_STATUS"
    fi
    printf 'vtcode: accepted source=Notarized Developer ID\n'
}

export VTCODE_MACOS_SIGNING_IDENTITY='Developer ID Application: Test Developer (TEAMID)'
export VTCODE_MACOS_NOTARY_PROFILE='VTCodeTestNotary'

TEST_UNAME=Linux
if macos_release_signing_preflight >/dev/null 2>&1; then
    fail_test 'preflight must reject macOS signing on non-macOS hosts'
else
    pass 'preflight rejects macOS signing on non-macOS hosts'
fi
unset TEST_UNAME

VTCODE_MACOS_SIGNING_IDENTITY='Apple Development: Test Developer (TEAMID)'
if macos_release_signing_preflight >/dev/null 2>&1; then
    fail_test 'preflight must reject a non-Developer-ID identity'
else
    pass 'preflight rejects a non-Developer-ID identity'
fi
VTCODE_MACOS_SIGNING_IDENTITY='Developer ID Application: Test Developer (TEAMID)'

TEST_IDENTITY_AVAILABLE=0
if macos_release_signing_preflight >/dev/null 2>&1; then
    fail_test 'preflight must reject an unavailable signing identity'
else
    pass 'preflight rejects an unavailable signing identity'
fi

TEST_IDENTITY_AVAILABLE=1
TEST_NOTARY_HISTORY_STATUS=1
if macos_release_signing_preflight >/dev/null 2>&1; then
    fail_test 'preflight must reject an unusable notary profile'
else
    pass 'preflight rejects an unusable notary profile'
fi
unset TEST_NOTARY_HISTORY_STATUS

if macos_release_signing_preflight; then
    pass 'preflight accepts a matching identity and notary profile'
else
    fail_test 'preflight should accept valid signing setup'
fi

if [[ "$VTCODE_MACOS_RELEASE_IDENTIFIER" == 'com.vinhnx.vtcode' ]]; then
    pass 'release code identifier is stable'
else
    fail_test "unexpected release code identifier: $VTCODE_MACOS_RELEASE_IDENTIFIER"
fi

printf 'macOS release binary fixture\n' >"$tmp/vtcode"
chmod +x "$tmp/vtcode"
if sign_and_notarize_macos_binary "$tmp/vtcode"; then
    pass 'accepted notarization is signed and Gatekeeper verified'
else
    fail_test 'accepted notarization should pass'
fi

if grep -Fq -- '--options' "$tmp/codesign-args" \
    && grep -Fq -- 'runtime' "$tmp/codesign-args" \
    && grep -Fq -- '--timestamp' "$tmp/codesign-args" \
    && grep -Fq -- 'com.vinhnx.vtcode' "$tmp/codesign-args"; then
    pass 'release signing enables hardened runtime, timestamp, and stable identifier'
else
    fail_test 'release signing flags are incomplete'
fi

mkdir -p "$tmp/archive-source"
cp "$tmp/vtcode" "$tmp/archive-source/vtcode"
tar -czf "$tmp/vtcode-release.tar.gz" -C "$tmp/archive-source" vtcode
if verify_macos_release_archive "$tmp/vtcode-release.tar.gz"; then
    pass 'release archive verification accepts a notarized root executable'
else
    fail_test 'release archive verification should pass'
fi

tar -czf "$tmp/vtcode-dot-release.tar.gz" -C "$tmp/archive-source" .
if verify_macos_release_archive "$tmp/vtcode-dot-release.tar.gz"; then
    pass 'release archive verification accepts the ./vtcode layout'
else
    fail_test 'release archive verification should accept the ./vtcode layout'
fi

mkdir -p "$tmp/nested-source/nested"
cp "$tmp/vtcode" "$tmp/nested-source/nested/vtcode"
tar -czf "$tmp/nested-release.tar.gz" -C "$tmp/nested-source" nested/vtcode
if verify_macos_release_archive "$tmp/nested-release.tar.gz" >/dev/null 2>&1; then
    fail_test 'release archive verification must reject a non-root executable'
else
    pass 'release archive verification rejects a non-root executable'
fi

printf 'not a tar archive\n' >"$tmp/invalid-release.tar.gz"
if verify_macos_release_archive "$tmp/invalid-release.tar.gz" >/dev/null 2>&1; then
    fail_test 'release archive verification must reject an unreadable archive'
else
    pass 'release archive verification rejects an unreadable archive'
fi

TEST_NOTARY_STATUS=Invalid
if sign_and_notarize_macos_binary "$tmp/vtcode" >/dev/null 2>&1; then
    fail_test 'invalid Apple notarization must block the release'
else
    pass 'invalid Apple notarization blocks the release'
fi
unset TEST_NOTARY_STATUS

TEST_SPCTL_STATUS=1
if verify_macos_release_binary "$tmp/vtcode" >/dev/null 2>&1; then
    fail_test 'Gatekeeper rejection must block the release'
else
    pass 'Gatekeeper rejection blocks the release'
fi

exit "$fail"
