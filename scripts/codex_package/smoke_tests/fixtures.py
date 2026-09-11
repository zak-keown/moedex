import gzip
import hashlib
import json
import os
import subprocess
import tarfile
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from typing import Self

import pytest
import zstandard
from app_server_harness import MockResponsesServer


def validate_extracted_package(package_root: Path, target: str) -> dict[str, Any]:
    manifest = json.loads((package_root / "codex-package.json").read_text())
    executable_suffix = ".exe" if "windows" in target else ""
    required_paths = [
        manifest["entrypoint"],
        f"bin/codex-code-mode-host{executable_suffix}",
        f"{manifest['pathDir']}/rg{executable_suffix}",
    ]
    if "linux" in target:
        required_paths.append(f"{manifest['resourcesDir']}/bwrap")
    elif "windows" in target:
        required_paths.extend(
            [
                f"{manifest['resourcesDir']}/codex-command-runner.exe",
                f"{manifest['resourcesDir']}/codex-windows-sandbox-setup.exe",
            ]
        )
    missing = [path for path in required_paths if not (package_root / path).is_file()]
    if missing:
        raise AssertionError(f"missing required package helpers: {', '.join(missing)}")

    checksums = manifest["checksums"]
    payloads = {
        path.relative_to(package_root).as_posix()
        for path in package_root.rglob("*")
        if path.is_file() and path.name != "codex-package.json"
    }
    assert set(checksums) == payloads, "checksum manifest does not cover every payload"
    for relative_path, expected_digest in checksums.items():
        actual_digest = hashlib.sha256(
            (package_root / relative_path).read_bytes()
        ).hexdigest()
        assert actual_digest == expected_digest, relative_path
    return manifest


@dataclass(frozen=True)
class SmokePackage:
    target: str
    cli: Path
    cli_root: Path
    cli_path_dir: Path
    app_server: Path
    app_server_root: Path
    app_server_path_dir: Path
    directory: Path
    environment: dict[str, str]

    @classmethod
    def from_archives(
        cls,
        directory: Path,
        target: str,
        cli_archive: Path,
        app_server_archive: Path,
        compression: str,
    ) -> Self:
        config_dir = directory / "codex-config"
        config_dir.mkdir()
        environment = dict(os.environ)
        conflicting_path = directory / "conflicting-path"
        conflicting_path.mkdir()
        for helper in ("moedex", "codex", "rg", "codex-code-mode-host"):
            suffix = ".exe" if "windows" in target else ""
            fake = conflicting_path / f"{helper}{suffix}"
            fake.write_text(
                "@echo off\r\nexit /b 91\r\n" if suffix else "#!/bin/sh\nexit 91\n"
            )
            fake.chmod(0o755)
        environment["PATH"] = os.pathsep.join(
            [str(conflicting_path), environment.get("PATH", "")]
        )

        # Preserve host proxies while routing the fake localhost model directly.
        inherited_bypass = (
            environment.get("NO_PROXY") or environment.get("no_proxy") or ""
        ).split(",")
        environment["NO_PROXY"] = ",".join(
            dict.fromkeys(
                host.strip()
                for host in [*inherited_bypass, "127.0.0.1", "localhost"]
                if host.strip()
            )
        )

        # Isolate package configuration and state from the user's Codex setup.
        environment["CODEX_HOME"] = str(config_dir)
        # Shell startup files can replace PATH and hide the packaged ripgrep.
        environment.pop("BASH_ENV", None)
        environment["ZDOTDIR"] = str(directory)

        extracted_packages = []
        for index, archive_path in enumerate((cli_archive, app_server_archive)):
            extracted = directory / f"package-{index}"
            extracted.mkdir()
            decompress = zstandard.open if compression == "zstd" else gzip.open
            with (
                decompress(archive_path, "rb") as source,
                tarfile.open(fileobj=source, mode="r|") as archive,
            ):
                archive.extractall(extracted, filter="data")
            manifest = validate_extracted_package(extracted, target)
            extracted_packages.append(
                (
                    extracted,
                    extracted / manifest["entrypoint"],
                    extracted / manifest["pathDir"],
                )
            )

        cli_root, cli, cli_path_dir = extracted_packages[0]
        app_server_root, app_server, app_server_path_dir = extracted_packages[1]

        (config_dir / "config.toml").write_text(
            """approval_policy = "never"
sandbox_mode = "workspace-write"
suppress_unstable_features_warning = true

[sandbox_workspace_write]
network_access = true

[features]
code_mode_only = true
code_mode_host = true
memories = false
apps = false
plugins = false

[analytics]
enabled = false

[otel]
exporter = "none"
trace_exporter = "none"
metrics_exporter = "none"
"""
        )
        return cls(
            target=target,
            cli=cli,
            cli_root=cli_root,
            cli_path_dir=cli_path_dir,
            app_server=app_server,
            app_server_root=app_server_root,
            app_server_path_dir=app_server_path_dir,
            directory=directory,
            environment=environment,
        )

    def run(
        self,
        *arguments: str,
        stdin: str | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(self.cli), *arguments],
            cwd=self.directory,
            env=self.environment,
            input=stdin,
            check=True,
            text=True,
            capture_output=True,
            timeout=45,
        )


@pytest.fixture(scope="session")
def package(
    request: pytest.FixtureRequest,
    tmp_path_factory: pytest.TempPathFactory,
) -> SmokePackage:
    compression = request.config.getoption("compression")
    target = request.config.getoption("package_target")
    directory = tmp_path_factory.mktemp(f"codex-package-smoke-{compression}")
    if "windows" in target:
        # pytest's 0700 directories have protected ACLs, so restricted-token
        # sandboxes need read/execute access to reach the packaged binaries.
        for path in (directory.parent, directory):
            subprocess.run(
                ["icacls", str(path), "/grant", "*S-1-1-0:(OI)(CI)(RX)"],
                check=True,
                capture_output=True,
                text=True,
            )
    return SmokePackage.from_archives(
        directory,
        target,
        request.config.getoption("cli_archive"),
        request.config.getoption("app_server_archive"),
        compression,
    )


@pytest.fixture
def responses_server(package: SmokePackage) -> Iterator[MockResponsesServer]:
    config_path = Path(package.environment["CODEX_HOME"]) / "config.toml"
    original_config = config_path.read_text()
    with MockResponsesServer() as server:
        # Direct CLI commands and SDK-launched servers must share a provider;
        # CodexConfig only controls how the SDK launches its server process.
        config_path.write_text(
            f"""model = "package-smoke"
model_provider = "package_smoke"

{original_config}

[model_providers.package_smoke]
name = "package smoke"
base_url = "{server.url}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"""
        )
        try:
            yield server
        finally:
            config_path.write_text(original_config)


@pytest.fixture(scope="session")
def code_mode_host_debug_symbols(
    pytestconfig: pytest.Config,
    tmp_path_factory: pytest.TempPathFactory,
) -> Path:
    target = pytestconfig.getoption("package_target")
    destination = tmp_path_factory.mktemp("codex-debug-symbols")
    return extract_code_mode_host_debug_symbols(
        target,
        pytestconfig.getoption("cli_symbols_archive"),
        pytestconfig.getoption("app_server_symbols_archive"),
        destination,
    )


def extract_code_mode_host_debug_symbols(
    target: str,
    cli_symbols_archive: Path,
    app_server_symbols_archive: Path | None,
    destination: Path,
) -> Path:
    if "windows" in target:
        expected = {
            "moedex",
            "codex-app-server",
            "codex-code-mode-host",
            "codex-command-runner",
            "codex-responses-api-proxy",
            "codex-windows-sandbox-setup",
        }
        host_symbols = _validate_symbols_archive(
            cli_symbols_archive,
            target,
            expected,
            destination,
            label="combined Windows",
        )
        assert host_symbols is not None
        return host_symbols

    host_symbols = _validate_symbols_archive(
        cli_symbols_archive,
        target,
        {"moedex", "codex-code-mode-host", "codex-responses-api-proxy"},
        destination,
        label="CLI",
    )
    assert host_symbols is not None
    assert app_server_symbols_archive is not None, "missing app-server symbols archive"
    _validate_symbols_archive(
        app_server_symbols_archive,
        f"{target}-app-server",
        {"codex-app-server", "codex-code-mode-host"},
        destination,
        label="app-server",
        extract_host=False,
    )
    return host_symbols


def _validate_symbols_archive(
    archive_path: Path,
    artifact_name: str,
    binaries: set[str],
    destination: Path,
    *,
    label: str,
    extract_host: bool = True,
) -> Path | None:
    root = f"codex-symbols-{artifact_name}"
    if "apple-darwin" in artifact_name:
        markers = {
            binary: f"/{binary}.dSYM/Contents/Resources/DWARF/" for binary in binaries
        }
    else:
        extension = "pdb" if "windows" in artifact_name else "debug"
        markers = {binary: f"/{binary}.{extension}" for binary in binaries}

    found: set[str] = set()
    symbol_path = None
    with tarfile.open(archive_path, "r|gz") as archive:
        for member in archive:
            if not member.isfile():
                continue
            assert member.name.startswith(f"{root}/"), (
                f"{label} symbols archive has wrong root: {member.name}"
            )
            for binary, marker in markers.items():
                if "apple-darwin" in artifact_name:
                    _, found_marker, dwarf_name = member.name.partition(marker)
                    matches = bool(
                        found_marker and dwarf_name and "/" not in dwarf_name
                    )
                else:
                    matches = member.name.endswith(marker)
                if not matches:
                    continue
                found.add(binary)
                if binary == "codex-code-mode-host" and extract_host:
                    archive.extract(member, destination, filter="data")
                    symbol_path = destination / member.name
                break
    assert found == binaries, f"{label} symbols mismatch: {found}"
    if extract_host:
        assert symbol_path is not None, f"{label} code-mode host symbols missing"
    return symbol_path
