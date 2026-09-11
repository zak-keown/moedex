#!/usr/bin/env python3
"""Reject stock-product names in Moedex-owned public command and help surfaces."""

from pathlib import Path
import re
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
PUBLIC_SURFACES = (
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
    "codex-rs/tui/src/app/agents_overview.rs",
    "codex-rs/tui/src/app/event_dispatch.rs",
    "codex-rs/tui/src/app/exit_summary.rs",
    "codex-rs/tui/src/app/startup.rs",
    "codex-rs/tui/src/app/thread_goal_actions.rs",
    "codex-rs/tui/src/bottom_pane/memories_settings_view.rs",
    "codex-rs/tui/src/history_cell/notices.rs",
    "codex-rs/tui/src/keymap.rs",
    "codex-rs/tui/src/session_archive_commands.rs",
    "codex-rs/tui/src/session_queue_commands.rs",
    "codex-rs/tui/src/session_start.rs",
    "codex-rs/tui/src/startup_orchestration.rs",
    "codex-rs/core/src/session_rollout_init_error.rs",
    "codex-rs/thread-store/src/local/rollout_lineage.rs",
)
FORBIDDEN = re.compile(
    r"(?i)(?:\bcodex (?:app-server|archive|delete|doctor|fork|migrate-rollouts|queue|resume|sandbox|unarchive|agents)\b|"
    r"\bCodex (?:config|home|keymap|couldn't start|rebuilt|detected|can rebuild|process|copies)\b|"
    r"another Codex process|~[/\\]\.codex[/\\]config\.toml)"
)


class PublicBrandInventoryTest(unittest.TestCase):
    def test_moedex_public_surfaces_do_not_leak_stock_product_names(self) -> None:
        leaks: list[str] = []
        for relative in PUBLIC_SURFACES:
            for line_number, line in enumerate(
                (REPO_ROOT / relative).read_text(encoding="utf-8").splitlines(), 1
            ):
                if match := FORBIDDEN.search(line):
                    leaks.append(f"{relative}:{line_number}: {match.group(0)}")
        self.assertEqual(leaks, [])


if __name__ == "__main__":
    unittest.main()
