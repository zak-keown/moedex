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
    def test_install_never_offers_or_runs_stock_codex_uninstall(self) -> None:
        script = INSTALL_SCRIPT.read_text(encoding="utf-8")

        self.assertNotIn("Uninstall the existing", script)
        self.assertNotIn('@("remove", "-g", "@openai/codex")', script)
        self.assertNotIn('@("uninstall", "-g", "@openai/codex")', script)

    def test_parser_exposes_uninstall_switch(self) -> None:
        result = run_powershell(
            "if (-not (Get-Command $env:MOEDEX_TEST_INSTALLER).Parameters"
            '.ContainsKey("Uninstall")) { exit 1 }'
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_public_uninstall_preserves_moedex_data_and_stock_codex(self) -> None:
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

            result = invoke_public_uninstall(moedex_home, visible_bin, root, stock_home)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(link_exists(visible_bin))
            self.assertFalse(link_exists(current))
            self.assertTrue(release.is_dir())
            self.assertEqual(moedex_data.read_text(), "retained conversation\n")
            self.assertEqual(stock_data.read_text(), "stock credentials\n")
            self.assertTrue(stock_binary.is_file())

    def test_public_uninstall_preserves_every_link_for_shared_codex_home(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            stock_home = root / "stock-home"
            visible_bin, current, release = create_installed_layout(stock_home, root)
            stock_data = stock_home / "auth.json"
            stock_data.write_text("stock credentials\n", encoding="utf-8")

            result = invoke_public_uninstall(stock_home, visible_bin, root, stock_home)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("shared with Codex", result.stdout + result.stderr)
            self.assertTrue(link_exists(visible_bin))
            self.assertTrue(link_exists(current))
            self.assertEqual(current.resolve(), release.resolve())
            self.assertTrue((visible_bin / "moedex.exe").is_file())
            self.assertTrue((visible_bin / "codex-code-mode-host.exe").is_file())
            self.assertEqual(stock_data.read_text(), "stock credentials\n")

    def test_public_uninstall_preserves_canonical_stock_home_alias(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            stock_home = root / "profile" / ".codex"
            visible_bin, current, release = create_installed_layout(stock_home, root)
            alias = root / "stock-home-alias"
            create_directory_link(alias, stock_home)

            result = invoke_public_uninstall(alias, visible_bin, root / "profile", "")

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("shared with Codex", result.stdout + result.stderr)
            self.assertTrue(link_exists(visible_bin))
            self.assertTrue(link_exists(current))
            self.assertEqual(current.resolve(), release.resolve())
            self.assertTrue((visible_bin / "codex-code-mode-host.exe").is_file())


def create_installed_layout(home: Path, root: Path) -> tuple[Path, Path, Path]:
    release = home / "packages" / "standalone" / "releases" / "test-release"
    release_bin = release / "bin"
    release_bin.mkdir(parents=True)
    (release_bin / "moedex.exe").write_text("moedex binary\n", encoding="utf-8")
    (release_bin / "codex-code-mode-host.exe").write_text(
        "code mode host\n", encoding="utf-8"
    )
    standalone = home / "packages" / "standalone"
    current = standalone / "current"
    create_directory_link(current, release)
    (standalone / "moedex-current-target").write_text(
        f"{release.absolute()}\n", encoding="utf-8"
    )
    visible_bin = root / "visible-bin"
    create_directory_link(visible_bin, current / "bin")
    return visible_bin, current, release


def create_directory_link(link: Path, target: Path) -> None:
    link.parent.mkdir(parents=True, exist_ok=True)
    if os.name == "nt":
        result = subprocess.run(
            ["cmd", "/d", "/c", "mklink", "/J", str(link), str(target.absolute())],
            capture_output=True,
            check=False,
            text=True,
            timeout=10,
        )
        if result.returncode != 0:
            raise AssertionError(result.stderr or result.stdout)
    else:
        link.symlink_to(target.absolute(), target_is_directory=True)


def link_exists(path: Path) -> bool:
    return path.is_symlink() or path.exists()


def invoke_public_uninstall(
    moedex_home: Path,
    visible_bin: Path,
    user_profile: Path,
    codex_home: Path | str,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.update(
        {
            "MOEDEX_HOME": str(moedex_home),
            "MOEDEX_INSTALL_DIR": str(visible_bin),
            "USERPROFILE": str(user_profile),
            "LOCALAPPDATA": str(user_profile / "AppData" / "Local"),
            "CODEX_HOME": str(codex_home),
        }
    )
    return subprocess.run(
        [
            str(POWERSHELL),
            "-NoLogo",
            "-NoProfile",
            "-File",
            str(INSTALL_SCRIPT.resolve()),
            "-Uninstall",
        ],
        capture_output=True,
        check=False,
        env=env,
        text=True,
        timeout=20,
    )


def run_powershell(command: str) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
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
