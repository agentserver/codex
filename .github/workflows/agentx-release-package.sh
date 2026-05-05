#!/usr/bin/env bash
# Package the built `codex` binary as `agentx` for release distribution.
#
# Inputs (env vars):
#   TARGET      Rust target triple (e.g. x86_64-unknown-linux-musl)
#   PLATFORM    One of: linux, macos, windows
#   OUTDIR      Output directory (created if needed)
#
# Reads:        codex-rs/target/${TARGET}/release/codex[.exe]
#               (macOS only) codex-rs/target/${TARGET}/release/codex-${TARGET}.dmg
#
# Produces:     ${OUTDIR}/agentx-${TARGET}.tar.gz                    (linux + macos)
#               ${OUTDIR}/agentx-${TARGET}.dmg                       (macos only)
#               ${OUTDIR}/agentx-${TARGET}.exe.zip                   (windows)
#               ${OUTDIR}/SHA256SUMS                                 (always; appended)

set -euo pipefail

: "${TARGET:?TARGET env var required}"
: "${PLATFORM:?PLATFORM env var required (linux|macos|windows)}"
: "${OUTDIR:?OUTDIR env var required}"

mkdir -p "$OUTDIR"

case "$PLATFORM" in
  linux|macos)
    src="codex-rs/target/${TARGET}/release/codex"
    [[ -f "$src" ]] || { echo "missing: $src"; exit 1; }

    workdir="$(mktemp -d)"
    cp "$src" "${workdir}/agentx"
    chmod +x "${workdir}/agentx"
    tar -C "$workdir" -czf "${OUTDIR}/agentx-${TARGET}.tar.gz" agentx
    rm -rf "$workdir"

    if [[ "$PLATFORM" == "macos" ]]; then
      dmg_src="codex-rs/target/${TARGET}/release/codex-${TARGET}.dmg"
      [[ -f "$dmg_src" ]] || { echo "missing: $dmg_src"; exit 1; }
      cp "$dmg_src" "${OUTDIR}/agentx-${TARGET}.dmg"
    fi
    ;;

  windows)
    src="codex-rs/target/${TARGET}/release/codex.exe"
    [[ -f "$src" ]] || { echo "missing: $src"; exit 1; }

    workdir="$(mktemp -d)"
    cp "$src" "${workdir}/agentx.exe"
    (cd "$workdir" && zip -q "${OUTDIR}/agentx-${TARGET}.exe.zip" agentx.exe)
    rm -rf "$workdir"
    ;;

  *)
    echo "unknown PLATFORM: $PLATFORM" >&2
    exit 2
    ;;
esac

# Refresh SHA256SUMS to cover everything currently in OUTDIR (except itself).
(
  cd "$OUTDIR"
  : > SHA256SUMS
  for f in $(find . -maxdepth 1 -type f ! -name 'SHA256SUMS' | sort); do
    sha256sum "$f" >> SHA256SUMS
  done
)
