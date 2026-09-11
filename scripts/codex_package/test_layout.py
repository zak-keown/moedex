#!/usr/bin/env python3

from pathlib import Path
import hashlib
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from codex_package.layout import build_package_dir
from codex_package.layout import refresh_package_manifest
from codex_package.layout import validate_package_dir
from codex_package.targets import PACKAGE_VARIANTS
from codex_package.targets import PackageInputs
from codex_package.targets import TARGET_SPECS


class PackageLayoutTest(unittest.TestCase):
    def test_release_manifest_rejects_unknown_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "moedex"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )
            variant = PACKAGE_VARIANTS["codex"]
            spec = TARGET_SPECS["x86_64-unknown-linux-musl"]

            with mock.patch.dict(
                os.environ,
                {
                    "MOEDEX_REQUIRE_PROVENANCE": "1",
                    "STABLE_GIT_COMMIT": "unknown",
                    "STABLE_UPSTREAM_GIT_COMMIT": "unknown",
                    "MOEDEX_RELEASE_CHANNEL": "github",
                },
                clear=False,
            ):
                with self.assertRaisesRegex(RuntimeError, "package provenance"):
                    build_package_dir(package_dir, "1.2.3", variant, spec, inputs)

    def test_manifest_records_provenance_and_every_payload_checksum(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "moedex"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )

            build_package_dir(
                package_dir,
                "1.2.3",
                PACKAGE_VARIANTS["codex"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                inputs,
            )
            manifest = json.loads((package_dir / "codex-package.json").read_text())

            self.assertEqual(manifest["provenance"]["repository"], "zak-keown/moedex")
            self.assertEqual(manifest["provenance"]["product"], "moedex")
            self.assertEqual(
                manifest["provenance"],
                {
                    "forkCommit": "unknown",
                    "product": "moedex",
                    "releaseChannel": "github",
                    "repository": "zak-keown/moedex",
                    "upstreamCommit": "unknown",
                },
            )
            checksums = manifest["checksums"]
            payloads = sorted(
                path.relative_to(package_dir).as_posix()
                for path in package_dir.rglob("*")
                if path.is_file() and path.name != "codex-package.json"
            )
            self.assertEqual(sorted(checksums), payloads)
            for relative_path in payloads:
                digest = hashlib.sha256(
                    (package_dir / relative_path).read_bytes()
                ).hexdigest()
                self.assertEqual(checksums[relative_path], digest)

    def test_validation_rejects_tampered_or_missing_release_helper(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "moedex"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )
            variant = PACKAGE_VARIANTS["codex"]
            spec = TARGET_SPECS["x86_64-unknown-linux-musl"]
            build_package_dir(package_dir, "1.2.3", variant, spec, inputs)
            (package_dir / "codex-resources" / "bwrap").unlink()

            with self.assertRaisesRegex(
                RuntimeError, "Missing package file: codex-resources/bwrap"
            ):
                validate_package_dir(package_dir, variant, spec, include_zsh=False)

    def test_refresh_manifest_covers_resources_added_after_initial_staging(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "moedex"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=None,
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )
            variant = PACKAGE_VARIANTS["codex"]
            spec = TARGET_SPECS["aarch64-apple-darwin"]
            build_package_dir(package_dir, "1.2.3", variant, spec, inputs)
            voice = package_dir / "codex-resources" / "voice" / "manifest.json"
            voice.parent.mkdir(parents=True)
            voice.write_text('{"version":"1.2.3"}\n')

            refresh_package_manifest(package_dir)
            validate_package_dir(package_dir, variant, spec, include_zsh=False)

            manifest = json.loads((package_dir / "codex-package.json").read_text())
            self.assertIn("codex-resources/voice/manifest.json", manifest["checksums"])

    def test_macos_package_preserves_prebuilt_resource_binaries(self) -> None:
        for variant_name in ("codex", "codex-app-server"):
            for target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
                with self.subTest(variant=variant_name, target=target):
                    with tempfile.TemporaryDirectory() as temp_dir:
                        root = Path(temp_dir)
                        package_dir = root / "package"
                        package_dir.mkdir()
                        rg_bin = touch_executable(root / "signed-rg")
                        zsh_bin = touch_executable(root / "signed-zsh")
                        rg_bin.write_bytes(b"signed ripgrep binary")
                        zsh_bin.write_bytes(b"signed zsh binary")
                        variant = PACKAGE_VARIANTS[variant_name]
                        spec = TARGET_SPECS[target]
                        inputs = PackageInputs(
                            entrypoint_bin=touch_executable(
                                root / variant.executable_stem
                            ),
                            code_mode_host_bin=touch_executable(
                                root / "codex-code-mode-host"
                            ),
                            rg_bin=rg_bin,
                            zsh_bin=zsh_bin,
                            bwrap_bin=None,
                            codex_command_runner_bin=None,
                            codex_windows_sandbox_setup_bin=None,
                        )

                        build_package_dir(package_dir, "1.2.3", variant, spec, inputs)
                        validate_package_dir(
                            package_dir, variant, spec, include_zsh=True
                        )

                        self.assertEqual(
                            {
                                "rg": (package_dir / "codex-path" / "rg").read_bytes(),
                                "zsh": (
                                    package_dir
                                    / "codex-resources"
                                    / "zsh"
                                    / "bin"
                                    / "zsh"
                                ).read_bytes(),
                            },
                            {
                                "rg": b"signed ripgrep binary",
                                "zsh": b"signed zsh binary",
                            },
                        )

    def test_app_server_package_places_code_mode_host_beside_entrypoint(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "codex-app-server"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )

            build_package_dir(
                package_dir,
                "1.2.3",
                PACKAGE_VARIANTS["codex-app-server"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                inputs,
            )
            validate_package_dir(
                package_dir,
                PACKAGE_VARIANTS["codex-app-server"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                include_zsh=False,
            )

            self.assertTrue((package_dir / "bin" / "codex-code-mode-host").is_file())


def touch_executable(path: Path) -> Path:
    path.touch(mode=0o755)
    return path


if __name__ == "__main__":
    unittest.main()
