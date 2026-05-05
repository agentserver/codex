#!/usr/bin/env bash
# Unit tests for agentx-release-package.sh.
# Each test: set up a fake codex binary in a temp tree, invoke the
# packaging script, assert on the output archive contents.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_SH="${SCRIPT_DIR}/../agentx-release-package.sh"

[[ -f "$PACKAGE_SH" ]] || { echo "FAIL: $PACKAGE_SH not found"; exit 1; }

passed=0
failed=0
fail() { echo "FAIL: $1"; failed=$((failed + 1)); }
pass() { echo "PASS: $1"; passed=$((passed + 1)); }

with_tempdir() {
  local d
  d="$(mktemp -d)"
  trap 'rm -rf "$d"' RETURN
  pushd "$d" > /dev/null
  "$@"
  popd > /dev/null
}

# Test 1: Linux tar.gz contains a binary named 'agentx' with the right contents.
test_linux_tarball() {
  local target="x86_64-unknown-linux-musl"
  mkdir -p "codex-rs/target/${target}/release"
  printf 'fake-binary-payload' > "codex-rs/target/${target}/release/codex"
  chmod +x "codex-rs/target/${target}/release/codex"

  TARGET="$target" PLATFORM=linux OUTDIR="$PWD/dist" bash "$PACKAGE_SH"

  local archive="dist/agentx-${target}.tar.gz"
  [[ -f "$archive" ]] || { fail "linux tarball missing: $archive"; return; }

  mkdir -p extract && tar -xzf "$archive" -C extract
  [[ -f extract/agentx ]] || { fail "tarball missing 'agentx' entry"; return; }
  [[ "$(cat extract/agentx)" == "fake-binary-payload" ]] \
    || { fail "tarball binary content mismatch"; return; }
  [[ -x extract/agentx ]] || { fail "tarball binary not executable"; return; }
  pass "linux tarball"
}

# Test 2: Windows .exe.zip contains 'agentx.exe' with the right contents.
test_windows_zip() {
  local target="x86_64-pc-windows-msvc"
  mkdir -p "codex-rs/target/${target}/release"
  printf 'fake-windows-payload' > "codex-rs/target/${target}/release/codex.exe"

  TARGET="$target" PLATFORM=windows OUTDIR="$PWD/dist" bash "$PACKAGE_SH"

  local archive="dist/agentx-${target}.exe.zip"
  [[ -f "$archive" ]] || { fail "windows zip missing: $archive"; return; }

  mkdir -p extract && unzip -q "$archive" -d extract
  [[ -f extract/agentx.exe ]] || { fail "zip missing 'agentx.exe' entry"; return; }
  [[ "$(cat extract/agentx.exe)" == "fake-windows-payload" ]] \
    || { fail "zip binary content mismatch"; return; }
  pass "windows zip"
}

# Test 3: macOS tar.gz contains 'agentx', and dmg passthrough works (we don't
# build a real dmg here; we mock its presence and assert it's renamed/copied).
test_macos_outputs() {
  local target="aarch64-apple-darwin"
  mkdir -p "codex-rs/target/${target}/release"
  printf 'fake-mac-binary' > "codex-rs/target/${target}/release/codex"
  chmod +x "codex-rs/target/${target}/release/codex"
  printf 'fake-dmg-content' > "codex-rs/target/${target}/release/codex-${target}.dmg"

  TARGET="$target" PLATFORM=macos OUTDIR="$PWD/dist" bash "$PACKAGE_SH"

  local tarball="dist/agentx-${target}.tar.gz"
  local dmg="dist/agentx-${target}.dmg"
  [[ -f "$tarball" ]] || { fail "macos tarball missing: $tarball"; return; }
  [[ -f "$dmg" ]] || { fail "macos dmg missing: $dmg"; return; }

  mkdir -p extract && tar -xzf "$tarball" -C extract
  [[ -f extract/agentx ]] || { fail "macos tarball missing 'agentx'"; return; }
  [[ "$(cat extract/agentx)" == "fake-mac-binary" ]] \
    || { fail "macos tarball binary content mismatch"; return; }
  [[ "$(cat "$dmg")" == "fake-dmg-content" ]] \
    || { fail "macos dmg content not preserved"; return; }
  pass "macos outputs"
}

# Test 4: SHA256SUMS file is generated and contains every artifact.
test_sha256sums() {
  local target="x86_64-unknown-linux-musl"
  mkdir -p "codex-rs/target/${target}/release"
  printf 'x' > "codex-rs/target/${target}/release/codex"
  chmod +x "codex-rs/target/${target}/release/codex"

  TARGET="$target" PLATFORM=linux OUTDIR="$PWD/dist" bash "$PACKAGE_SH"

  local sums="dist/SHA256SUMS"
  [[ -f "$sums" ]] || { fail "SHA256SUMS missing"; return; }
  grep -q "agentx-${target}.tar.gz" "$sums" \
    || { fail "SHA256SUMS missing tarball entry"; return; }
  pass "sha256sums"
}

with_tempdir test_linux_tarball
with_tempdir test_windows_zip
with_tempdir test_macos_outputs
with_tempdir test_sha256sums

echo "Results: ${passed} passed, ${failed} failed"
[[ $failed -eq 0 ]] || exit 1
