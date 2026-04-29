#!/usr/bin/env python3

"""Verify codex-tui stays behind the app-server/core boundary."""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TUI_ROOT = ROOT / "codex-rs" / "tui"
TUI_MANIFEST = TUI_ROOT / "Cargo.toml"
FORBIDDEN_PACKAGE = "codex-core"
FORBIDDEN_SOURCE_RULES = (
    (
        "imports `codex_core`",
        (
            re.compile(r"\bcodex_core::"),
            re.compile(r"\buse\s+codex_core\b"),
            re.compile(r"\bextern\s+crate\s+codex_core\b"),
        ),
    ),
    (
        "references `codex_protocol::protocol`",
        (
            re.compile(r"\bcodex_protocol\s*::\s*protocol\b"),
            re.compile(r"\bcodex_protocol\s*::\s*\{[^}]*\bprotocol\b"),
        ),
    ),
)
CODEX_PROTOCOL_ALIAS_PATTERNS = (
    re.compile(r"\buse\s+codex_protocol\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\s*;"),
    re.compile(
        r"\bextern\s+crate\s+codex_protocol\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\s*;"
    ),
)


def main() -> int:
    failures = []
    failures.extend(manifest_failures())
    failures.extend(source_failures())

    if not failures:
        return 0

    print("codex-tui must stay behind the app-server/core boundary.")
    print(
        "Use app-server protocol types at the TUI boundary; temporary embedded "
        "startup gaps belong behind codex_app_server_client::legacy_core, and "
        "core protocol references should remain outside codex-tui."
    )
    print()
    for failure in failures:
        print(f"- {failure}")

    return 1


def manifest_failures() -> list[str]:
    manifest = tomllib.loads(TUI_MANIFEST.read_text())
    failures = []
    for section_name, dependencies in dependency_sections(manifest):
        if FORBIDDEN_PACKAGE in dependencies:
            failures.append(
                f"{relative_path(TUI_MANIFEST)} declares `{FORBIDDEN_PACKAGE}` "
                f"in `[{section_name}]`"
            )
    return failures


def dependency_sections(manifest: dict) -> list[tuple[str, dict]]:
    sections: list[tuple[str, dict]] = []
    for section_name in ("dependencies", "dev-dependencies", "build-dependencies"):
        dependencies = manifest.get(section_name)
        if isinstance(dependencies, dict):
            sections.append((section_name, dependencies))

    for target_name, target in manifest.get("target", {}).items():
        if not isinstance(target, dict):
            continue
        for section_name in ("dependencies", "dev-dependencies", "build-dependencies"):
            dependencies = target.get(section_name)
            if isinstance(dependencies, dict):
                sections.append((f'target.{target_name}.{section_name}', dependencies))

    return sections


def source_failures() -> list[str]:
    failures = []
    for path in sorted(TUI_ROOT.glob("**/*.rs")):
        text = path.read_text()
        seen_locations = set()
        for message, patterns in FORBIDDEN_SOURCE_RULES:
            for pattern in patterns:
                for match in pattern.finditer(text):
                    failures.append(source_failure(path, text, match.start(), message))
                    seen_locations.add((match.start(), message))

        for alias_match in alias_matches(text):
            alias = re.escape(alias_match.group(1))
            patterns = (
                re.compile(rf"\b{alias}\s*::\s*protocol\b"),
                re.compile(rf"\b{alias}\s*::\s*\{{[^}}]*\bprotocol\b"),
            )
            for pattern in patterns:
                for match in pattern.finditer(text):
                    key = (match.start(), "references `codex_protocol::protocol`")
                    if key in seen_locations:
                        continue
                    failures.append(
                        source_failure(
                            path, text, match.start(), "references `codex_protocol::protocol`"
                        )
                    )
                    seen_locations.add(key)
    return failures


def alias_matches(text: str) -> list[re.Match[str]]:
    matches = []
    for pattern in CODEX_PROTOCOL_ALIAS_PATTERNS:
        matches.extend(pattern.finditer(text))
    return matches


def source_failure(path: Path, text: str, offset: int, message: str) -> str:
    line_number = text.count("\n", 0, offset) + 1
    return f"{relative_path(path)}:{line_number} {message}"


def relative_path(path: Path) -> str:
    return str(path.relative_to(ROOT))


if __name__ == "__main__":
    sys.exit(main())
