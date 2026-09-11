#!/usr/bin/env python3
"""Reject stock-product names in Moedex-owned public command and help surfaces."""

from pathlib import Path
import re
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
EXPLICIT_PUBLIC_SURFACES = (
    "codex-rs/config/src/tui_keymap.rs",
    "codex-rs/utils/cli/src/config_override.rs",
    "codex-rs/utils/cli/src/resume_command.rs",
    "codex-rs/cli/src/state_db_recovery.rs",
    "codex-rs/cli/src/doctor.rs",
    "codex-rs/cli/src/doctor/background.rs",
    "codex-rs/cli/src/doctor/desktop.rs",
    "codex-rs/cli/src/doctor/desktop/macos_security.rs",
    "codex-rs/cli/src/doctor/sandbox.rs",
    "codex-rs/cli/src/migrate_rollouts.rs",
    "codex-rs/cli/src/queue_cmd.rs",
    "codex-rs/cli/src/sandbox_setup.rs",
    "codex-rs/app-server/src/lib.rs",
    "codex-rs/app-server/src/request_processors/thread_processor.rs",
    "codex-rs/app-server/src/request_processors/thread_queue_processor.rs",
    "codex-rs/core/src/session_rollout_init_error.rs",
    "codex-rs/thread-store/src/local/rollout_lineage.rs",
    "codex-rs/exec/src/lib.rs",
    "codex-rs/codex-mcp/src/connection_manager/startup.rs",
    "codex-rs/app-server-transport/src/transport/websocket.rs",
    "codex-rs/app-server-daemon/README.md",
    "codex-rs/app-server-test-client/README.md",
    "codex-rs/exec-server/README.md",
    "codex-rs/exec-server/src/environment_toml.rs",
    "codex-rs/linux-sandbox/README.md",
    "codex-rs/login/src/assets/error.html",
    "codex-rs/responses-api-proxy/README.md",
    "codex-rs/skills/src/assets/samples/openai-docs/references/mcp-diagnostics.md",
    "codex-rs/skills/src/assets/samples/plugin-creator/SKILL.md",
    "codex-rs/skills/src/assets/samples/plugin-creator/references/installing-and-updating.md",
    ".github/ISSUE_TEMPLATE/3-cli.yml",
    "scripts/debug-codex.sh",
    "scripts/mcp_conformance/README.md",
    "scripts/run_tui_with_exec_server.sh",
    "scripts/start-codex-exec.sh",
    "scripts/test-remote-env.sh",
)
APPLICATION_PUBLIC_ROOTS = (
    "codex-rs/tui/src",
    "codex-rs/tui/assets",
    "codex-rs/app-server-daemon/src",
    "codex-rs/cloud-tasks/src",
    "codex-rs/chatgpt/src",
    "codex-rs/features/src",
    "codex-rs/protocol/src",
)
BASE_FORBIDDEN = re.compile(
    r"(?i)(?:\bcodex (?:app-server|archive|cloud|delete|doctor|exec|fork|login|mcp|migrate-rollouts|plugin|queue|resume|sandbox|unarchive|agents)\b|"
    r"\bCodex (?:config|home|keymap|login|couldn't start|rebuilt|detected|can rebuild|process|copies)\b|"
    r"another Codex process|~[/\\]\.codex[/\\]config\.toml|"
    r"--bin codex(?![-A-Za-z0-9_])|target[/\\]debug[/\\]codex(?![-A-Za-z0-9_]))"
)
TUI_FORBIDDEN = re.compile(
    r"(?i)(?:(?-i:\bcodex) (?:(?:app|mcp|resume|fork|exec|login|doctor|app-server)\b|"
    r"--[a-z][a-z0-9-]*\b|-[a-z]\b)|"
    r"\bCodex (?:CLI|service|application|binary|client|agent|can|could|will|asks|"
    r"performs|process|copies|now uses|may add|is currently|just got)\b|"
    r"\bcodex (?:could|to|network access)\b|"
    r"\b(?:ask|tell|grant|restart|run|exit|using|start|starting) Codex\b|"
    r"(?-i:[\"']Codex[\"']))"
)
TUI_ALLOWED = (
    re.compile(r"\bOpenAI Codex\b"),
    re.compile(r"\bCodex (?:extension|Cloud|Desktop|community forum)\b"),
    re.compile(r"\bCodex MCP\b"),
    re.compile(r"\bCodex keymap documentation\b"),
    re.compile(r"\bCodex App (?:directives|avatar catalog)\b"),
    re.compile(r"\bCodex-optimized\b"),
    re.compile(r"\bCodex is included in your plan\b"),
    re.compile(r"(?:~[/\\])?\.codex[/\\]config\.toml"),
    re.compile(r"\bcodex home (?:path|directory|file)\b", re.IGNORECASE),
)


def tui_forbidden() -> tuple[re.Pattern[str], ...]:
    return (BASE_FORBIDDEN, TUI_FORBIDDEN)


def without_tui_allowances(line: str) -> str:
    for pattern in TUI_ALLOWED:
        line = pattern.sub("", line)
    return line


def public_surfaces() -> list[Path]:
    surfaces = [REPO_ROOT / relative for relative in EXPLICIT_PUBLIC_SURFACES]
    for relative_root in APPLICATION_PUBLIC_ROOTS:
        root = REPO_ROOT / relative_root
        surfaces.extend(
            path
            for path in root.rglob("*")
            if path.suffix in {".rs", ".txt"}
            and "snapshots" not in path.parts
            and "tests" not in path.parts
            and path.name != "tests.rs"
            and not path.name.endswith("_tests.rs")
        )
    return sorted(set(surfaces))


def production_lines(path: Path) -> list[str]:
    lines = path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?mod tests\s*\{", line):
            return lines[:index]
    return lines


class PublicBrandInventoryTest(unittest.TestCase):
    def test_tui_pattern_rejects_public_stock_command_leaks(self) -> None:
        leaks = (
            "Run codex resume to continue.",
            "Use codex exec for automation.",
            "Run codex login first.",
            "Try codex doctor for diagnostics.",
            "Start codex app-server locally.",
            "Launch codex --profile work.",
            "Select one with codex -m gpt-5.5.",
            "The Codex config could not be loaded.",
        )
        for line in leaks:
            with self.subTest(line=line):
                self.assertTrue(
                    any(pattern.search(line) for pattern in tui_forbidden())
                )

    def test_tui_pattern_allows_compatibility_and_upstream_terms(self) -> None:
        allowed = (
            "OpenAI Codex",
            "Codex extension",
            "Codex Cloud task",
            "Codex Desktop protocol",
            "Codex-optimized model",
            "Codex is included in your plan",
            "[tui.keymap] in ~/.codex/config.toml",
            "See the Codex keymap documentation",
            "Codex App directives",
            "codex home path",
        )
        for line in allowed:
            with self.subTest(line=line):
                public_text = without_tui_allowances(line)
                self.assertFalse(
                    any(pattern.search(public_text) for pattern in tui_forbidden())
                )

    def test_moedex_public_surfaces_do_not_leak_stock_product_names(self) -> None:
        leaks: list[str] = []
        for path in public_surfaces():
            relative = path.relative_to(REPO_ROOT)
            is_application_root = any(
                str(relative).startswith(f"{root}/")
                for root in APPLICATION_PUBLIC_ROOTS
            )
            forbidden = tui_forbidden() if is_application_root else (BASE_FORBIDDEN,)
            for line_number, line in enumerate(production_lines(path), 1):
                line = without_tui_allowances(line)
                for pattern in forbidden:
                    if match := pattern.search(line):
                        leaks.append(f"{relative}:{line_number}: {match.group(0)}")
                        break
        self.assertEqual(leaks, [])


if __name__ == "__main__":
    unittest.main()
