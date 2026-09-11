#!/usr/bin/env python3

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import textwrap
import unittest


INSTALL_SCRIPT = Path(__file__).with_name("install.sh")
VERSION = "0.142.5"
MISMATCH_VERSION = "0.145.0"


class InstallShTest(unittest.TestCase):
    def test_install_never_offers_or_runs_stock_codex_uninstall(self) -> None:
        script = INSTALL_SCRIPT.read_text(encoding="utf-8")

        self.assertNotIn("Uninstall the existing", script)
        self.assertNotIn("brew uninstall --cask codex", script)
        self.assertNotIn("bun remove -g @openai/codex", script)
        self.assertNotIn("npm uninstall -g @openai/codex", script)

    def test_uninstall_preserves_moedex_data_and_stock_codex(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive, checksum, metadata = create_package_release(root)
            moedex_home = root / "moedex-home"
            stock_home = root / "stock-codex-home"
            stock_home.mkdir()
            stock_data = stock_home / "auth.json"
            stock_data.write_text("stock credentials\n", encoding="utf-8")
            stock_binary = root / "bin" / "codex"
            stock_binary.parent.mkdir(exist_ok=True)
            write_executable(stock_binary, "#!/bin/sh\nprintf 'codex-cli 9.9.9\\n'\n")

            installed, _ = run_installer_in(
                root,
                VERSION,
                metadata_json=metadata,
                archive_path=archive,
                checksum_path=checksum,
                force_macos=True,
                moedex_home=moedex_home,
                codex_home=stock_home,
            )
            self.assertEqual(installed.returncode, 0, installed.stderr)
            moedex_data = moedex_home / "history.jsonl"
            moedex_data.write_text("retained conversation\n", encoding="utf-8")
            releases = moedex_home / "packages" / "standalone" / "releases"
            self.assertTrue((root / "install-bin" / "moedex").is_symlink())
            (root / "requests.log").unlink()

            uninstalled, requests = run_installer_in(
                root,
                VERSION,
                force_macos=True,
                moedex_home=moedex_home,
                codex_home=stock_home,
                arguments=("--uninstall",),
            )

            self.assertEqual(uninstalled.returncode, 0, uninstalled.stderr)
            self.assertEqual(requests, [])
            self.assertFalse((root / "install-bin" / "moedex").exists())
            self.assertFalse((root / "install-bin" / "moedex").is_symlink())
            self.assertFalse((root / "install-bin" / "codex-code-mode-host").exists())
            self.assertFalse(
                (root / "install-bin" / "codex-code-mode-host").is_symlink()
            )
            self.assertFalse(
                (moedex_home / "packages" / "standalone" / "current").exists()
            )
            self.assertTrue(any(releases.iterdir()))
            self.assertEqual(moedex_data.read_text(), "retained conversation\n")
            self.assertTrue(stock_binary.exists())
            self.assertEqual(stock_data.read_text(), "stock credentials\n")

    def test_uninstall_does_not_mutate_shared_codex_home_package_links(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shared_home = root / "shared-home"
            current = shared_home / "packages" / "standalone" / "current"
            release = shared_home / "packages" / "standalone" / "releases" / "stock"
            release.mkdir(parents=True)
            current.symlink_to(release)
            install_bin = root / "install-bin"
            install_bin.mkdir()
            (install_bin / "moedex").symlink_to(current / "bin" / "moedex")
            (install_bin / "codex-code-mode-host").symlink_to(
                current / "bin" / "codex-code-mode-host"
            )

            result, requests = run_installer_in(
                root,
                VERSION,
                force_macos=True,
                codex_home=shared_home,
                arguments=("--uninstall",),
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(requests, [])
            self.assertTrue((install_bin / "moedex").is_symlink())
            self.assertTrue((install_bin / "codex-code-mode-host").is_symlink())
            self.assertTrue(current.is_symlink())
            self.assertEqual(current.resolve(), release.resolve())

    def test_moedex_home_alias_of_codex_home_never_removes_stock_current(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            stock_home = root / "stock-home"
            release = stock_home / "packages" / "standalone" / "releases" / "stock"
            release.mkdir(parents=True)
            current = stock_home / "packages" / "standalone" / "current"
            current.symlink_to(release)
            owner_marker = (
                stock_home / "packages" / "standalone" / "moedex-current-target"
            )
            owner_marker.write_text(f"{release.resolve()}\n", encoding="utf-8")
            stock_data = stock_home / "auth.json"
            stock_data.write_text("stock credentials\n", encoding="utf-8")
            install_bin = root / "install-bin"
            install_bin.mkdir()
            (install_bin / "moedex").symlink_to(current / "bin" / "moedex")
            (install_bin / "codex-code-mode-host").symlink_to(
                current / "bin" / "codex-code-mode-host"
            )

            result, requests = run_installer_in(
                root,
                VERSION,
                force_macos=True,
                moedex_home=stock_home,
                codex_home=stock_home,
                arguments=("--uninstall",),
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(requests, [])
            self.assertTrue((install_bin / "moedex").is_symlink())
            self.assertTrue((install_bin / "codex-code-mode-host").is_symlink())
            self.assertTrue(current.is_symlink())
            self.assertEqual(current.resolve(), release.resolve())
            self.assertEqual(stock_data.read_text(), "stock credentials\n")

    def test_installer_uses_only_the_moedex_github_repository(self) -> None:
        script = INSTALL_SCRIPT.read_text()
        self.assertIn("github.com/zak-keown/moedex", script)
        self.assertIn("api.github.com/repos/zak-keown/moedex", script)
        self.assertNotIn("github.com/openai/codex", script)
        self.assertNotIn("releases.openai.com", script)

    def test_metadata_fetch_failure_is_not_reported_as_missing_assets(self) -> None:
        result, requests = run_installer(VERSION, metadata_failure=True)

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            requests,
            [
                "https://api.github.com/repos/zak-keown/moedex/releases/tags/"
                f"rust-v{VERSION}"
            ],
        )
        self.assertIn(
            f"Could not fetch GitHub release metadata for Moedex {VERSION}",
            result.stderr,
        )
        self.assertNotIn("Could not find Moedex package", result.stderr)

    def test_exact_release_opt_out_uses_github_metadata_once(self) -> None:
        result, requests = run_installer(VERSION)

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            requests,
            [
                "https://api.github.com/repos/zak-keown/moedex/releases/tags/"
                f"rust-v{VERSION}",
                "https://github.com/zak-keown/moedex/releases/download/"
                f"rust-v{VERSION}/codex-package_SHA256SUMS",
            ],
        )
        self.assertIn(f"Resolved version: {VERSION}", result.stdout)

    def test_alpha_hotfix_release_is_valid(self) -> None:
        version = "0.145.0-alpha.23.1"
        result, requests = run_installer(version)

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            requests,
            [
                "https://api.github.com/repos/zak-keown/moedex/releases/tags/"
                f"rust-v{version}",
                "https://github.com/zak-keown/moedex/releases/download/"
                f"rust-v{version}/codex-package_SHA256SUMS",
            ],
        )
        self.assertIn(f"Resolved version: {version}", result.stdout)

    def test_latest_release_reuses_version_metadata(self) -> None:
        result, requests = run_installer("latest")

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            requests,
            [
                "https://api.github.com/repos/zak-keown/moedex/releases/latest",
                "https://github.com/zak-keown/moedex/releases/download/"
                f"rust-v{VERSION}/codex-package_SHA256SUMS",
            ],
        )
        self.assertIn(f"Resolved version: {VERSION}", result.stdout)

    def test_compact_metadata_is_independent_of_field_order(self) -> None:
        result, requests = run_installer(
            "latest", metadata_json=release_metadata(compact=True, reorder=True)
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            requests,
            [
                "https://api.github.com/repos/zak-keown/moedex/releases/latest",
                "https://github.com/zak-keown/moedex/releases/download/"
                f"rust-v{VERSION}/codex-package_SHA256SUMS",
            ],
        )
        self.assertIn(f"Resolved version: {VERSION}", result.stdout)

    def test_json_like_strings_and_nested_fields_do_not_define_assets(self) -> None:
        result, requests = run_installer(
            VERSION, metadata_json=legacy_release_metadata_with_decoys()
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(requests), 2)
        self.assertIn("/codex-npm-", requests[1])
        self.assertNotIn("codex-package_SHA256SUMS", requests[1])

    def test_macos_install_exposes_code_mode_host_beside_moedex(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, checksum_path, metadata_json = create_package_release(root)

            result, _requests = run_installer_in(
                root,
                VERSION,
                metadata_json=metadata_json,
                archive_path=archive_path,
                checksum_path=checksum_path,
                force_macos=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            install_bin = root / "install-bin"
            current = root / "codex-home" / "packages" / "standalone" / "current"
            codex_path = install_bin / "moedex"
            host_path = install_bin / "codex-code-mode-host"
            self.assertEqual(os.readlink(codex_path), str(current / "bin" / "moedex"))
            self.assertEqual(
                os.readlink(host_path),
                str(current / "bin" / "codex-code-mode-host"),
            )
            self.assertTrue(os.access(host_path, os.X_OK))

    def test_github_latest_installs_verified_package_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, checksum_path, metadata_json = create_package_release(root)

            result, requests = run_installer_in(
                root,
                "latest",
                metadata_json=metadata_json,
                archive_path=archive_path,
                checksum_path=checksum_path,
                force_macos=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                requests,
                [
                    "https://api.github.com/repos/zak-keown/moedex/releases/latest",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{VERSION}/codex-package_SHA256SUMS",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{VERSION}/codex-package-aarch64-apple-darwin.tar.gz",
                ],
            )

    def test_install_cleans_only_recognized_stale_moedex_command_links(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, checksum_path, metadata_json = create_package_release(root)
            install_bin = root / "install-bin"
            install_bin.mkdir()
            stock_temporary = install_bin / ".codex.interrupted"
            stock_temporary.write_text("stock installer state\n", encoding="utf-8")
            user_moedex_file = install_bin / ".moedex.notes"
            user_moedex_file.write_text("user data\n", encoding="utf-8")
            stale_moedex_link = install_bin / ".moedex.interrupted"
            stale_moedex_link.symlink_to(
                root
                / "codex-home"
                / "packages"
                / "standalone"
                / "current"
                / "bin"
                / "moedex"
            )

            result, _requests = run_installer_in(
                root,
                VERSION,
                metadata_json=metadata_json,
                archive_path=archive_path,
                checksum_path=checksum_path,
                force_macos=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(stock_temporary.read_text(), "stock installer state\n")
            self.assertEqual(user_moedex_file.read_text(), "user data\n")
            self.assertFalse(stale_moedex_link.exists())
            self.assertFalse(stale_moedex_link.is_symlink())

    def test_explicit_release_pins_even_the_current_latest_version(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive, checksum, metadata = create_package_release(root)
            options = dict(
                metadata_json=metadata,
                archive_path=archive,
                checksum_path=checksum,
                force_macos=True,
            )
            marker = root / "codex-home/packages/standalone/auto-update-version"
            latest, _ = run_installer_in(root, "latest", **options)
            self.assertEqual(latest.returncode, 0, latest.stderr)
            release_name = f"{VERSION}-aarch64-apple-darwin"
            self.assertEqual(marker.read_text(), release_name)

            pinned, _ = run_installer_in(root, VERSION, **options)
            self.assertEqual(pinned.returncode, 0, pinned.stderr)
            self.assertFalse(marker.exists())

            updater_record = (
                root / "codex-home/app-server-daemon/app-server-updater.pid"
            )
            updater_record.parent.mkdir(parents=True)
            updater_record.write_text(
                json.dumps(
                    {"pid": os.getpid(), "processStartTime": process_start_time()}
                )
            )
            old_updater, _ = run_installer_in(
                root, "latest", old_updater_parent_pid=os.getpid(), **options
            )
            self.assertEqual(old_updater.returncode, 0, old_updater.stderr)
            self.assertFalse(marker.exists())

            skipped, _ = run_installer_in(
                root, "latest", update_guard_from_release=release_name, **options
            )
            self.assertEqual(skipped.returncode, 0, skipped.stderr)
            self.assertFalse(marker.exists())

            updater_record.write_text(
                json.dumps({"pid": os.getpid(), "processStartTime": "stale"})
            )
            latest_again, _ = run_installer_in(
                root, "latest", old_updater_parent_pid=os.getpid(), **options
            )
            self.assertEqual(latest_again.returncode, 0, latest_again.stderr)
            self.assertEqual(marker.read_text(), release_name)

            managed = (
                root
                / f"codex-home/packages/standalone/releases/{release_name}/bin/moedex"
            )
            managed.unlink()
            guarded, _ = run_installer_in(
                root,
                "latest",
                update_guard_from_release=release_name,
                **options,
            )
            self.assertEqual(guarded.returncode, 0, guarded.stderr)
            self.assertTrue(managed.exists())

    def test_uninspectable_legacy_updater_does_not_clear_pin(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive, checksum, metadata = create_package_release(root)
            options = dict(
                metadata_json=metadata,
                archive_path=archive,
                checksum_path=checksum,
                force_macos=True,
            )
            pinned, _ = run_installer_in(root, VERSION, **options)
            self.assertEqual(pinned.returncode, 0, pinned.stderr)
            updater_record = (
                root / "codex-home/app-server-daemon/app-server-updater.pid"
            )
            updater_record.parent.mkdir(parents=True)
            updater_record.write_text(
                json.dumps(
                    {"pid": os.getpid(), "processStartTime": process_start_time()}
                )
            )

            attempted, _ = run_installer_in(
                root,
                "latest",
                old_updater_parent_pid=os.getpid(),
                fail_ps=True,
                **options,
            )
            self.assertEqual(attempted.returncode, 0, attempted.stderr)
            self.assertFalse(
                (root / "codex-home/packages/standalone/auto-update-version").exists()
            )

    def test_github_corrupt_checksum_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, checksum_path, metadata_json = create_package_release(root)

            result, requests = run_installer_in(
                root,
                "latest",
                metadata_json=metadata_json,
                archive_path=archive_path,
                checksum_path=checksum_path,
                force_macos=True,
                github_mode="corrupt_checksum",
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(
                requests,
                [
                    "https://api.github.com/repos/zak-keown/moedex/releases/latest",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{VERSION}/codex-package_SHA256SUMS",
                ],
            )
            self.assertIn("checksum did not match expected digest", result.stderr)

    def test_github_incomplete_checksum_manifest_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, _, metadata_json = create_package_release(root)
            checksum_path = root / "incomplete-SHA256SUMS"
            checksum_path.write_text(
                f"{'a' * 64}  codex-package-other-platform.tar.gz\n",
                encoding="utf-8",
            )
            metadata = json.loads(metadata_json)
            for asset in metadata["assets"]:
                if asset["name"] == "codex-package_SHA256SUMS":
                    asset["digest"] = (
                        "sha256:"
                        + hashlib.sha256(checksum_path.read_bytes()).hexdigest()
                    )

            result, requests = run_installer_in(
                root,
                "latest",
                metadata_json=json.dumps(metadata),
                archive_path=archive_path,
                checksum_path=checksum_path,
                force_macos=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(
                requests,
                [
                    "https://api.github.com/repos/zak-keown/moedex/releases/latest",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{VERSION}/codex-package_SHA256SUMS",
                ],
            )
            self.assertIn("Could not find SHA-256 digest", result.stderr)

    def test_github_exact_release_rejects_wrong_binary_version(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, checksum_path, metadata_json = create_package_release(
                root,
                metadata_version=MISMATCH_VERSION,
            )

            result, requests = run_installer_in(
                root,
                MISMATCH_VERSION,
                metadata_json=metadata_json,
                archive_path=archive_path,
                checksum_path=checksum_path,
                force_macos=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(
                requests,
                [
                    "https://api.github.com/repos/zak-keown/moedex/releases/tags/"
                    f"rust-v{MISMATCH_VERSION}",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{MISMATCH_VERSION}/codex-package_SHA256SUMS",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{MISMATCH_VERSION}/codex-package-aarch64-apple-darwin.tar.gz",
                ],
            )
            self.assertIn(
                f"did not report expected version {MISMATCH_VERSION}",
                result.stderr,
            )
            self.assertNotIn("installed successfully", result.stdout)

    def test_github_exact_legacy_release_reuses_offline_install(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive_path, metadata_json = create_legacy_release(root)

            first_result, first_requests = run_installer_in(
                root,
                VERSION,
                metadata_json=metadata_json,
                legacy_archive_path=archive_path,
                force_macos=True,
            )

            self.assertEqual(first_result.returncode, 0, first_result.stderr)
            self.assertEqual(
                first_requests,
                [
                    "https://api.github.com/repos/zak-keown/moedex/releases/tags/"
                    f"rust-v{VERSION}",
                    "https://github.com/zak-keown/moedex/releases/download/"
                    f"rust-v{VERSION}/codex-npm-darwin-arm64-{VERSION}.tgz",
                ],
            )

            (root / "requests.log").unlink()
            second_result, second_requests = run_installer_in(
                root,
                VERSION,
                metadata_json=metadata_json,
                force_macos=True,
            )

            self.assertEqual(second_result.returncode, 0, second_result.stderr)
            self.assertEqual(
                second_requests,
                [
                    "https://api.github.com/repos/zak-keown/moedex/releases/tags/"
                    f"rust-v{VERSION}"
                ],
            )
            self.assertNotIn("Downloading Moedex CLI", second_result.stdout)


def run_installer(
    release: str,
    *,
    metadata_failure: bool = False,
    metadata_json: str | None = None,
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    with tempfile.TemporaryDirectory() as temp_dir:
        return run_installer_in(
            Path(temp_dir),
            release,
            metadata_failure=metadata_failure,
            metadata_json=metadata_json,
        )


def run_installer_in(
    root: Path,
    release: str,
    *,
    metadata_failure: bool = False,
    metadata_json: str | None = None,
    archive_path: Path | None = None,
    checksum_path: Path | None = None,
    legacy_archive_path: Path | None = None,
    force_macos: bool = False,
    github_mode: str = "",
    update_guard_from_release: str | None = None,
    old_updater_parent_pid: int | None = None,
    fail_ps: bool = False,
    moedex_home: Path | None = None,
    codex_home: Path | None = None,
    arguments: tuple[str, ...] = (),
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    bin_dir = root / "bin"
    bin_dir.mkdir(exist_ok=True)
    request_log = root / "requests.log"
    fake_curl = bin_dir / "curl"
    fake_curl.write_text(
        textwrap.dedent(
            """\
            #!/bin/sh
            url=""
            output=""
            previous=""
            for arg in "$@"; do
              case "$arg" in
                https://*) url="$arg" ;;
              esac
              if [ "$previous" = "-o" ]; then
                output="$arg"
              fi
              previous="$arg"
            done
            printf '%s\n' "$url" >>"$CODEX_TEST_REQUEST_LOG"

            case "$url" in
              https://api.github.com/*)
                if [ "$CODEX_TEST_METADATA_FAILURE" = "1" ]; then
                  echo "curl: (22) The requested URL returned error: 403" >&2
                  exit 22
                fi
                printf '%s\n' "$CODEX_TEST_METADATA_JSON"
                ;;
              https://github.com/zak-keown/moedex/releases/download/*/codex-package_SHA256SUMS)
                if [ "$CODEX_TEST_GITHUB_MODE" = "corrupt_checksum" ]; then
                  printf '<html>proxy error</html>\n' >"$output"
                  exit 0
                fi
                if [ -n "$CODEX_TEST_CHECKSUM_PATH" ]; then
                  cp "$CODEX_TEST_CHECKSUM_PATH" "$output"
                else
                  exit 22
                fi
                ;;
              https://github.com/zak-keown/moedex/releases/download/*/codex-package-*.tar.gz)
                if [ -n "$CODEX_TEST_ARCHIVE_PATH" ]; then
                  cp "$CODEX_TEST_ARCHIVE_PATH" "$output"
                else
                  exit 22
                fi
                ;;
              https://github.com/zak-keown/moedex/releases/download/*/codex-npm-*.tgz)
                if [ -n "$CODEX_TEST_LEGACY_ARCHIVE_PATH" ]; then
                  cp "$CODEX_TEST_LEGACY_ARCHIVE_PATH" "$output"
                else
                  exit 22
                fi
                ;;
              *)
                exit 22
                ;;
            esac
            """
        ),
        encoding="utf-8",
    )
    fake_curl.chmod(0o755)
    if force_macos:
        fake_uname = bin_dir / "uname"
        fake_uname.write_text(
            "#!/bin/sh\n"
            'case "$1" in\n'
            "  -s) printf 'Darwin\\n' ;;\n"
            "  -m) printf 'arm64\\n' ;;\n"
            "esac\n",
            encoding="utf-8",
        )
        fake_uname.chmod(0o755)
    if old_updater_parent_pid is not None:
        fake_ps = bin_dir / "ps"
        fake_ps.write_text(
            "#!/bin/sh\nexit 1\n"
            if fail_ps
            else "#!/bin/sh\n"
            'case "$*" in\n'
            '  *lstart*) printf "S %s\\n" "$CODEX_TEST_PARENT_START" ;;\n'
            '  *) printf "%s\\n" "$CODEX_TEST_PARENT_PID" ;;\n'
            "esac\n",
            encoding="utf-8",
        )
        fake_ps.chmod(0o755)

    home = root / "home"
    home.mkdir(exist_ok=True)
    env = os.environ.copy()
    env.update(
        {
            "CODEX_HOME": str(codex_home or root / "codex-home"),
            "CODEX_INSTALL_DIR": str(root / "install-bin"),
            "CODEX_NON_INTERACTIVE": "1",
            "CODEX_RELEASE": release,
            "CODEX_TEST_ARCHIVE_PATH": str(archive_path or ""),
            "CODEX_TEST_CHECKSUM_PATH": str(checksum_path or ""),
            "CODEX_TEST_LEGACY_ARCHIVE_PATH": str(legacy_archive_path or ""),
            "CODEX_TEST_GITHUB_MODE": github_mode,
            "CODEX_TEST_METADATA_FAILURE": "1" if metadata_failure else "0",
            "CODEX_TEST_METADATA_JSON": (
                metadata_json if metadata_json is not None else release_metadata()
            ),
            "CODEX_TEST_REQUEST_LOG": str(request_log),
            "HOME": str(home),
            "PATH": f"{bin_dir}:/usr/bin:/bin",
            "SHELL": "/bin/sh",
        }
    )
    if moedex_home is not None:
        env["MOEDEX_HOME"] = str(moedex_home)
    else:
        env.pop("MOEDEX_HOME", None)
    if update_guard_from_release is None:
        env.pop("CODEX_INSTALL_IF_LATEST", None)
        env.pop("CODEX_UPDATE_FROM_RELEASE", None)
    else:
        env["CODEX_INSTALL_IF_LATEST"] = "1"
        env["CODEX_UPDATE_FROM_RELEASE"] = update_guard_from_release
    if old_updater_parent_pid is not None:
        env["CODEX_TEST_PARENT_PID"] = str(old_updater_parent_pid)
        env["CODEX_TEST_PARENT_START"] = process_start_time()
    result = subprocess.run(
        ["/bin/sh", str(INSTALL_SCRIPT), *arguments],
        capture_output=True,
        check=False,
        env=env,
        text=True,
    )
    requests = (
        request_log.read_text(encoding="utf-8").splitlines()
        if request_log.exists()
        else []
    )
    return result, requests


def process_start_time() -> str:
    details = subprocess.check_output(
        ["ps", "-p", str(os.getpid()), "-o", "stat=", "-o", "lstart="], text=True
    ).strip()
    return details.split(maxsplit=1)[1]


def create_package_release(
    root: Path,
    *,
    metadata_version: str = VERSION,
) -> tuple[Path, Path, str]:
    package_dir = root / "package"
    (package_dir / "bin").mkdir(parents=True)
    (package_dir / "codex-path").mkdir()
    (package_dir / "codex-package.json").write_text("{}\n", encoding="utf-8")
    write_executable(
        package_dir / "bin" / "moedex",
        f"#!/bin/sh\nprintf 'codex-cli {VERSION}\\n'\n",
    )
    write_executable(
        package_dir / "bin" / "codex-code-mode-host",
        "#!/bin/sh\nexit 0\n",
    )
    write_executable(package_dir / "codex-path" / "rg", "#!/bin/sh\nexit 0\n")

    asset = "codex-package-aarch64-apple-darwin.tar.gz"
    archive_path = root / asset
    with tarfile.open(archive_path, "w:gz") as archive:
        for path in package_dir.iterdir():
            archive.add(path, arcname=path.name)

    archive_digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    checksum_path = root / "codex-package_SHA256SUMS"
    checksum_path.write_text(f"{archive_digest}  {asset}\n", encoding="utf-8")
    checksum_digest = hashlib.sha256(checksum_path.read_bytes()).hexdigest()
    metadata_json = json.dumps(
        {
            "assets": [
                {"name": asset, "digest": f"sha256:{archive_digest}"},
                {
                    "name": "codex-package_SHA256SUMS",
                    "digest": f"sha256:{checksum_digest}",
                },
            ],
            "tag_name": f"rust-v{metadata_version}",
        },
        indent=2,
    )
    return archive_path, checksum_path, metadata_json


def create_legacy_release(root: Path) -> tuple[Path, str]:
    package_dir = root / "legacy-package"
    vendor_dir = package_dir / "package" / "vendor" / "aarch64-apple-darwin"
    (vendor_dir / "bin").mkdir(parents=True)
    (vendor_dir / "path").mkdir()
    write_executable(
        vendor_dir / "bin" / "moedex",
        f"#!/bin/sh\nprintf 'codex-cli {VERSION}\\n'\n",
    )
    write_executable(vendor_dir / "path" / "rg", "#!/bin/sh\nexit 0\n")

    asset = f"codex-npm-darwin-arm64-{VERSION}.tgz"
    archive_path = root / asset
    with tarfile.open(archive_path, "w:gz") as archive:
        archive.add(package_dir / "package", arcname="package")

    archive_digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    metadata_json = json.dumps(
        {
            "assets": [{"name": asset, "digest": f"sha256:{archive_digest}"}],
            "tag_name": f"rust-v{VERSION}",
        },
        indent=2,
    )
    return archive_path, metadata_json


def write_executable(path: Path, contents: str) -> None:
    path.write_text(contents, encoding="utf-8")
    path.chmod(0o755)


def release_metadata(*, compact: bool = False, reorder: bool = False) -> str:
    assets = [
        asset_metadata(
            f"codex-package-{target}.tar.gz",
            f"sha256:{'a' * 64}",
            reorder=reorder,
        )
        for target in (
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "aarch64-unknown-linux-musl",
            "x86_64-unknown-linux-musl",
        )
    ]
    assets.append(
        asset_metadata(
            "codex-package_SHA256SUMS",
            f"sha256:{'b' * 64}",
            reorder=reorder,
        )
    )
    separators = (",", ":") if compact else None
    return json.dumps(
        {"assets": assets, "body": "braces: { } [ ]", "tag_name": f"rust-v{VERSION}"},
        indent=None if compact else 2,
        separators=separators,
    )


def asset_metadata(name: str, digest: str, *, reorder: bool) -> dict[str, str]:
    if reorder:
        return {"digest": digest, "name": name}
    return {"name": name, "digest": digest}


def legacy_release_metadata_with_decoys() -> str:
    fake_digest = f"sha256:{'0' * 64}"
    assets = [
        {
            "metadata": {
                "name": "codex-package-x86_64-unknown-linux-musl.tar.gz",
                "digest": fake_digest,
            },
            "digest": f"sha256:{'c' * 64}",
            "name": f"codex-npm-{target}-{VERSION}.tgz",
        }
        for target in ("darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64")
    ]
    return json.dumps(
        {
            "body": (
                f'fake: {{"name":"codex-package_SHA256SUMS","digest":"{fake_digest}"}}'
            ),
            "assets": assets,
            "tag_name": f"rust-v{VERSION}",
        },
        separators=(",", ":"),
    )


if __name__ == "__main__":
    unittest.main()
