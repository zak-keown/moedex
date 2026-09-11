#!/usr/bin/env python3

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


INSTALL_SCRIPT = Path(__file__).with_name("install.ps1")
POWERSHELL = shutil.which("pwsh") or shutil.which("powershell")


@unittest.skipIf(POWERSHELL is None, "PowerShell is not installed")
class InstallPs1Test(unittest.TestCase):
    def test_parser_exposes_uninstall_switch(self) -> None:
        result = run_powershell(
            "if (-not (Get-Command $env:MOEDEX_TEST_INSTALLER).Parameters"
            '.ContainsKey("Uninstall")) { exit 1 }'
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_uninstall_preserves_moedex_data_and_stock_codex(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            moedex_home = root / "moedex-home"
            stock_home = root / "stock-home"
            visible_bin, current, release = create_installed_layout(moedex_home, root)
            moedex_data = moedex_home / "history.jsonl"
            moedex_data.write_text("retained conversation\n", encoding="utf-8")
            stock_home.mkdir()
            stock_data = stock_home / "auth.json"
            stock_data.write_text("stock credentials\n", encoding="utf-8")
            stock_binary = root / "stock-bin" / "codex.exe"
            stock_binary.parent.mkdir()
            stock_binary.write_text("stock binary\n", encoding="utf-8")

            result = invoke_uninstall(moedex_home, visible_bin, root, stock_home)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(visible_bin.is_symlink())
            self.assertFalse(current.is_symlink())
            self.assertTrue(release.is_dir())
            self.assertEqual(moedex_data.read_text(), "retained conversation\n")
            self.assertEqual(stock_data.read_text(), "stock credentials\n")
            self.assertTrue(stock_binary.is_file())

    def test_moedex_home_alias_of_codex_home_preserves_stock_current(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            stock_home = root / "stock-home"
            visible_bin, current, release = create_installed_layout(stock_home, root)
            stock_data = stock_home / "auth.json"
            stock_data.write_text("stock credentials\n", encoding="utf-8")

            result = invoke_uninstall(stock_home, visible_bin, root, stock_home)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(visible_bin.is_symlink())
            self.assertTrue(current.is_symlink())
            self.assertEqual(current.resolve(), release.resolve())
            self.assertEqual(stock_data.read_text(), "stock credentials\n")


def create_installed_layout(home: Path, root: Path) -> tuple[Path, Path, Path]:
    release = home / "packages" / "standalone" / "releases" / "test-release"
    release_bin = release / "bin"
    release_bin.mkdir(parents=True)
    (release_bin / "moedex.exe").write_text("moedex binary\n", encoding="utf-8")
    standalone = home / "packages" / "standalone"
    current = standalone / "current"
    current.symlink_to(release, target_is_directory=True)
    (standalone / "moedex-current-target").write_text(
        f"{release.absolute()}\n", encoding="utf-8"
    )
    visible_bin = root / "visible-bin"
    visible_bin.symlink_to(current / "bin", target_is_directory=True)
    return visible_bin, current, release


def invoke_uninstall(
    moedex_home: Path,
    visible_bin: Path,
    user_profile: Path,
    codex_home: Path,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.update(
        {
            "MOEDEX_TEST_HOME": str(moedex_home),
            "MOEDEX_TEST_VISIBLE_BIN": str(visible_bin),
            "MOEDEX_TEST_USERPROFILE": str(user_profile),
            "MOEDEX_TEST_CODEX_HOME": str(codex_home),
        }
    )
    return run_powershell(
        ". $env:MOEDEX_TEST_INSTALLER; "
        "Uninstall-Moedex -MoedexHome $env:MOEDEX_TEST_HOME "
        "-VisibleBinDir $env:MOEDEX_TEST_VISIBLE_BIN "
        "-UserProfile $env:MOEDEX_TEST_USERPROFILE "
        "-CodexHome $env:MOEDEX_TEST_CODEX_HOME",
        env=env,
    )


def run_powershell(
    command: str, *, env: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy() if env is None else env
    environment["MOEDEX_TEST_INSTALLER"] = str(INSTALL_SCRIPT.resolve())
    return subprocess.run(
        [str(POWERSHELL), "-NoLogo", "-NoProfile", "-Command", command],
        capture_output=True,
        check=False,
        env=environment,
        text=True,
        timeout=20,
    )


if __name__ == "__main__":
    unittest.main()
