import io
import tarfile
from pathlib import Path

import pytest

from fixtures import extract_code_mode_host_debug_symbols


TARGET = "x86_64-unknown-linux-musl"


def write_symbols_archive(
    path: Path,
    artifact_name: str,
    binaries: set[str],
    *,
    extension: str = "debug",
) -> Path:
    with tarfile.open(path, "w:gz") as archive:
        for binary in sorted(binaries):
            data = f"symbols for {binary}".encode()
            info = tarfile.TarInfo(
                f"codex-symbols-{artifact_name}/{binary}.{extension}"
            )
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return path


def test_corrupt_cli_symbols_archive_fails(tmp_path: Path) -> None:
    cli = tmp_path / "cli.tar.gz"
    cli.write_bytes(b"not a symbols archive")
    app_server = write_symbols_archive(
        tmp_path / "app-server.tar.gz",
        f"{TARGET}-app-server",
        {"codex-app-server", "codex-code-mode-host"},
    )

    with pytest.raises(tarfile.ReadError):
        extract_code_mode_host_debug_symbols(
            TARGET, cli, app_server, tmp_path / "extracted"
        )


def test_wrong_cli_symbols_archive_fails(tmp_path: Path) -> None:
    cli = write_symbols_archive(
        tmp_path / "cli.tar.gz",
        TARGET,
        {"codex", "codex-code-mode-host", "codex-responses-api-proxy"},
    )
    app_server = write_symbols_archive(
        tmp_path / "app-server.tar.gz",
        f"{TARGET}-app-server",
        {"codex-app-server", "codex-code-mode-host"},
    )

    with pytest.raises(AssertionError, match="CLI symbols"):
        extract_code_mode_host_debug_symbols(
            TARGET, cli, app_server, tmp_path / "extracted"
        )


def test_wrong_app_server_symbols_archive_fails(tmp_path: Path) -> None:
    cli = write_symbols_archive(
        tmp_path / "cli.tar.gz",
        TARGET,
        {"moedex", "codex-code-mode-host", "codex-responses-api-proxy"},
    )
    app_server = write_symbols_archive(
        tmp_path / "app-server.tar.gz",
        TARGET,
        {"moedex", "codex-code-mode-host", "codex-responses-api-proxy"},
    )

    with pytest.raises(AssertionError, match="app-server symbols"):
        extract_code_mode_host_debug_symbols(
            TARGET, cli, app_server, tmp_path / "extracted"
        )


def test_corrupt_app_server_symbols_archive_fails(tmp_path: Path) -> None:
    cli = write_symbols_archive(
        tmp_path / "cli.tar.gz",
        TARGET,
        {"moedex", "codex-code-mode-host", "codex-responses-api-proxy"},
    )
    app_server = tmp_path / "app-server.tar.gz"
    app_server.write_bytes(b"not a symbols archive")

    with pytest.raises(tarfile.ReadError):
        extract_code_mode_host_debug_symbols(
            TARGET, cli, app_server, tmp_path / "extracted"
        )


def test_valid_split_symbols_return_packaged_host_symbols(tmp_path: Path) -> None:
    cli = write_symbols_archive(
        tmp_path / "cli.tar.gz",
        TARGET,
        {"moedex", "codex-code-mode-host", "codex-responses-api-proxy"},
    )
    app_server = write_symbols_archive(
        tmp_path / "app-server.tar.gz",
        f"{TARGET}-app-server",
        {"codex-app-server", "codex-code-mode-host"},
    )

    symbols = extract_code_mode_host_debug_symbols(
        TARGET, cli, app_server, tmp_path / "extracted"
    )

    assert symbols.read_bytes() == b"symbols for codex-code-mode-host"


def test_valid_combined_windows_symbols_include_every_bundle(tmp_path: Path) -> None:
    target = "x86_64-pc-windows-msvc"
    combined = write_symbols_archive(
        tmp_path / "combined.tar.gz",
        target,
        {
            "moedex",
            "codex-app-server",
            "codex-code-mode-host",
            "codex-command-runner",
            "codex-responses-api-proxy",
            "codex-windows-sandbox-setup",
        },
        extension="pdb",
    )

    symbols = extract_code_mode_host_debug_symbols(
        target, combined, combined, tmp_path / "extracted"
    )

    assert symbols.read_bytes() == b"symbols for codex-code-mode-host"
