#!/usr/bin/env python3

import copy
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import io

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import moedex_behavior_manifest as behavior


UPSTREAM_BASE = "8e2afc09126c0cea4c282725fe68af43adad73d7"
VOICE_PLUGINS = (
    "app",
    "audioconvert",
    "audioresample",
    "coreelements",
    "opus",
    "rtp",
    "rtpmanager",
)


def repository_manifest() -> dict:
    return behavior.load_manifest(
        Path(__file__).resolve().parents[1] / "moedex-behavior-manifest.json"
    )


def fixture_manifest() -> dict:
    manifest = copy.deepcopy(repository_manifest())
    manifest["sourceInventory"] = {
        "files": ["surface.txt", "second.txt"],
        "discoveryGlobs": ["*.txt"],
        "policyScan": {
            "roots": [".github"],
            "extensions": [".txt", ".yml"],
            "excludedDirectories": ["vendor"],
            "excludedFileGlobs": ["test_*"],
            "forbiddenRegex": [
                r"https?://(?:api\.)?github\.com/(?:repos/)?openai/codex/(?:releases|tags|latest)"
            ],
            "allowedOccurrences": [],
        },
        "expectations": [
            {
                "path": "surface.txt",
                "literal": "github.com/zak-keown/moedex",
                "count": 1,
            },
            {"path": "second.txt", "literal": "release: disabled", "count": 1},
            {
                "path": "second.txt",
                "literal": "github.com/zak-keown/moedex",
                "count": 0,
            },
        ],
        "forbiddenRegex": [
            {"path": "surface.txt", "pattern": r"github\.com/openai/codex/releases"},
            {"path": "second.txt", "pattern": r"npm publish"},
        ],
    }
    return manifest


def write_manifest(root: Path, manifest: dict) -> Path:
    path = root / "manifest.json"
    path.write_text(json.dumps(manifest), encoding="utf-8")
    return path


def write_package(
    root: Path, entrypoint: str, target: str = "x86_64-unknown-linux-musl"
) -> None:
    suffix = ".exe" if "windows" in target else ""
    payloads = [
        entrypoint,
        f"bin/codex-code-mode-host{suffix}",
        f"codex-path/rg{suffix}",
    ]
    if "linux" in target:
        payloads.append("codex-resources/bwrap")
    if "windows" in target:
        payloads.extend(
            [
                "codex-resources/codex-command-runner.exe",
                "codex-resources/codex-windows-sandbox-setup.exe",
            ]
        )
    if "apple-darwin" in target:
        payloads.append("codex-resources/zsh/bin/zsh")
        if Path(entrypoint).name == "moedex":
            voice_files = [
                "codex-resources/voice/bin/codex-voice-host",
                "codex-resources/voice/NOTICE.md",
                "codex-resources/voice/sources.json",
                "codex-resources/voice/lib/libgstreamer-1.0.0.dylib",
                "codex-resources/voice/lib/libgio-2.0.0.dylib",
                "codex-resources/voice/licenses/LGPL-2.1.txt",
                "codex-resources/voice/licenses/Opus.txt",
                "codex-resources/voice/licenses/PCRE2.md",
                "codex-resources/voice/licenses/libffi.txt",
                "codex-resources/voice/licenses/proxy-libintl.txt",
                "codex-resources/voice/licenses/sljit.txt",
                "codex-resources/voice/licenses/zlib.txt",
            ]
            voice_files.extend(
                f"codex-resources/voice/plugins/libgst{name}.dylib"
                for name in VOICE_PLUGINS
            )
            payloads.extend(voice_files)
    for relative in payloads:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(relative, encoding="utf-8")
        path.chmod(0o755)
    if "apple-darwin" in target and Path(entrypoint).name == "moedex":
        voice_root = root / "codex-resources/voice"
        libraries = []
        for relative in [
            "lib/libgstreamer-1.0.0.dylib",
            "lib/libgio-2.0.0.dylib",
            *(f"plugins/libgst{name}.dylib" for name in VOICE_PLUGINS),
        ]:
            path = voice_root / relative
            libraries.append(
                {
                    "path": relative,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            )
        runtime = {
            "schemaVersion": 1,
            "developmentOnly": False,
            "distribution": "publicRelease",
            "target": target,
            "sourceCommit": "f" * 40,
            "sourceManifestSha256": hashlib.sha256(
                (behavior.REPO_ROOT / "third_party/voice/sources.json").read_bytes()
            ).hexdigest(),
            "plugins": sorted(f"plugins/libgst{name}.dylib" for name in VOICE_PLUGINS),
            "libraries": libraries,
        }
        runtime_path = voice_root / "runtime.json"
        runtime_path.write_text(json.dumps(runtime), encoding="utf-8")
        payloads.append("codex-resources/voice/runtime.json")
        voice_payloads = [
            relative
            for relative in payloads
            if relative.startswith("codex-resources/voice/")
        ]
        voice_manifest = {
            "schemaVersion": 1,
            "buildCommit": "f" * 40,
            "appTarget": target,
            "voiceTarget": target,
            "appVersion": "0.1.0",
            "sha256": {
                relative: hashlib.sha256((root / relative).read_bytes()).hexdigest()
                for relative in [entrypoint, *voice_payloads]
            },
        }
        voice_manifest_path = root / "codex-resources/voice/manifest.json"
        voice_manifest_path.write_text(json.dumps(voice_manifest), encoding="utf-8")
        payloads.append("codex-resources/voice/manifest.json")
    metadata = {
        "layoutVersion": 1,
        "version": "0.1.0",
        "target": target,
        "variant": "codex"
        if Path(entrypoint).name in {"moedex", "moedex.exe"}
        else "codex-app-server",
        "entrypoint": entrypoint,
        "resourcesDir": "codex-resources",
        "pathDir": "codex-path",
        "provenance": {
            "product": "moedex",
            "repository": "zak-keown/moedex",
            "forkCommit": "f" * 40,
            "upstreamCommit": UPSTREAM_BASE,
            "releaseChannel": "github",
        },
        "checksums": {
            relative: hashlib.sha256((root / relative).read_bytes()).hexdigest()
            for relative in payloads
        },
    }
    (root / "codex-package.json").write_text(json.dumps(metadata), encoding="utf-8")


def artifact_fixture(root: Path) -> Path:
    artifact_root = root / "artifacts"
    write_package(artifact_root / "cli", "bin/moedex")
    write_package(artifact_root / "app-server", "bin/codex-app-server")
    return artifact_root


def write_symbols(path: Path, root_name: str, names: list[str], extension: str) -> Path:
    with tarfile.open(path, "w:gz") as archive:
        for name in names:
            payload = name.encode()
            member = tarfile.TarInfo(f"{root_name}/{name}.{extension}")
            member.size = len(payload)
            archive.addfile(member, io.BytesIO(payload))
    return path


def source_fixture(root: Path) -> None:
    (root / "surface.txt").write_text("github.com/zak-keown/moedex")
    (root / "second.txt").write_text("release: disabled")
    (root / ".github/workflows").mkdir(parents=True)


def test_repository_manifest_is_complete_and_strict() -> None:
    manifest = repository_manifest()
    assert behavior.validate_manifest(manifest) == []
    assert [entry["id"] for entry in manifest["requirements"]] == list(
        behavior.REQUIRED_IDS
    )
    assert all("exclusions" in entry for entry in manifest["requirements"])


def test_repository_policy_scans_all_github_automation_with_exact_v8_allowance() -> (
    None
):
    policy = repository_manifest()["sourceInventory"]["policyScan"]
    assert ".github" in policy["roots"]
    assert any(
        item["path"] == ".github/actions/setup-rusty-v8/action.yml"
        and item["count"] == 1
        for item in policy["allowedOccurrences"]
    )


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        (lambda manifest: manifest.update(extra=True), "unknown field in manifest"),
        (
            lambda manifest: manifest["requirements"][0].update(status="maybe"),
            "unknown status",
        ),
        (
            lambda manifest: manifest["requirements"][0].update(upstreamBase="0" * 40),
            "upstreamBase does not match",
        ),
        (
            lambda manifest: manifest["gates"]["identity"].update(argv=[]),
            "argv must be a nonempty",
        ),
        (
            lambda manifest: manifest["gates"]["identity"].update(cwd="../escape"),
            "unsafe gate identity cwd",
        ),
        (
            lambda manifest: manifest["requirements"][0]["tests"].append("missing"),
            "references missing gate",
        ),
        (
            lambda manifest: manifest["requirements"].append(
                copy.deepcopy(manifest["requirements"][0])
            ),
            "duplicate requirement ids",
        ),
        (
            lambda manifest: manifest["requirements"][0]["artifacts"][0][
                "paths"
            ].append("../escape"),
            "unsafe B1 artifact[0] path",
        ),
    ],
)
def test_schema_mutations_fail(mutation, message: str) -> None:
    manifest = fixture_manifest()
    mutation(manifest)
    assert any(message in error for error in behavior.validate_manifest(manifest))


def test_non_broken_requirement_must_have_real_mapping() -> None:
    manifest = fixture_manifest()
    manifest["requirements"][1]["tests"] = []
    manifest["requirements"][1]["artifacts"] = []
    assert "B2 must map to a gate or artifact" in behavior.validate_manifest(manifest)


def test_preserved_or_superseded_requirement_requires_executable_gate() -> None:
    manifest = fixture_manifest()
    entry = next(item for item in manifest["requirements"] if item["id"] == "B10")
    entry["tests"] = []
    entry["artifacts"] = [{"paths": ["cli/bin/version.txt"], "executable": False}]
    assert (
        "B10 preserved behavior requires an executable gate"
        in behavior.validate_manifest(manifest)
    )


def test_b1_and_b9_map_the_packaged_archive_journeys() -> None:
    manifest = repository_manifest()
    by_id = {entry["id"]: entry for entry in manifest["requirements"]}
    assert "package-journeys" in by_id["B1"]["tests"]
    assert "package-journeys" in by_id["B9"]["tests"]


def test_broken_requirement_never_certifies(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    entry = manifest["requirements"][0]
    entry["status"] = "broken"
    entry["reason"] = "fixture breakage"
    source_fixture(tmp_path)
    errors, _ = behavior.verify(write_manifest(tmp_path, manifest), tmp_path)
    assert "manifest contains a broken requirement" in errors


def test_duplicate_artifact_paths_fail() -> None:
    manifest = fixture_manifest()
    manifest["requirements"][1]["artifacts"] = [
        copy.deepcopy(manifest["requirements"][0]["artifacts"][0])
    ]
    assert any(
        "duplicate artifact path" in error
        for error in behavior.validate_manifest(manifest)
    )


def test_duplicate_json_gate_definition_fails_to_load(tmp_path: Path) -> None:
    path = tmp_path / "manifest.json"
    path.write_text('{"gates":{"same":{},"same":{}}}', encoding="utf-8")
    with pytest.raises(ValueError, match="duplicate JSON field: same"):
        behavior.load_manifest(path)


def test_source_inventory_is_an_exact_multiset_and_rejects_upstream_adoption(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    source_fixture(tmp_path)
    source = tmp_path / "surface.txt"
    assert behavior.verify_source_inventory(manifest, tmp_path) == []

    source.write_text("github.com/zak-keown/moedex\ngithub.com/zak-keown/moedex")
    assert any(
        "source occurrence mismatch" in error
        for error in behavior.verify_source_inventory(manifest, tmp_path)
    )

    source.write_text("")
    (tmp_path / "second.txt").write_text(
        "release: disabled\ngithub.com/zak-keown/moedex"
    )
    errors = behavior.verify_source_inventory(manifest, tmp_path)
    assert sum("source occurrence mismatch" in error for error in errors) == 2

    source.write_text(
        "github.com/zak-keown/moedex\nhttps://github.com/openai/codex/releases/latest"
    )
    assert any(
        "forbidden upstream adoption target" in error
        for error in behavior.verify_source_inventory(manifest, tmp_path)
    )


def test_policy_scan_rejects_realistically_named_new_publish_workflow(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    source_fixture(tmp_path)
    workflow = tmp_path / ".github/workflows/publish.yml"
    workflow.parent.mkdir(parents=True, exist_ok=True)
    workflow.write_text(
        "https://github.com/openai/codex/releases/latest", encoding="utf-8"
    )

    errors = behavior.verify_source_inventory(manifest, tmp_path)

    assert any("policy scan forbidden target" in error for error in errors)


def test_policy_scan_rejects_new_reusable_publish_action(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    source_fixture(tmp_path)
    action = tmp_path / ".github/actions/publish/action.yml"
    action.parent.mkdir(parents=True)
    action.write_text(
        "run: curl https://github.com/openai/codex/releases/latest",
        encoding="utf-8",
    )

    errors = behavior.verify_source_inventory(manifest, tmp_path)

    assert any("policy scan forbidden target" in error for error in errors)


def test_policy_scan_rejects_symlinked_file_escaping_repository(
    tmp_path: Path,
) -> None:
    if os.name == "nt":
        pytest.skip("symlink creation is not generally available")
    manifest = fixture_manifest()
    source_fixture(tmp_path)
    outside = tmp_path.parent / f"{tmp_path.name}-outside.yml"
    outside.write_text(
        "run: curl https://github.com/openai/codex/releases/latest",
        encoding="utf-8",
    )
    symlink = tmp_path / ".github/actions/escape/action.yml"
    symlink.parent.mkdir(parents=True)
    symlink.symlink_to(outside)

    errors = behavior.verify_source_inventory(manifest, tmp_path)

    assert any("policy scan file is a symlink" in error for error in errors)


def test_upstream_rebase_mutations_cannot_silently_drop_a_mapped_behavior(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    del manifest["gates"][manifest["requirements"][0]["tests"][0]]
    errors = behavior.validate_manifest(manifest)
    assert any("references missing gate" in error for error in errors)

    manifest = fixture_manifest()
    manifest["requirements"][0]["tests"] = []
    manifest["requirements"][0]["artifacts"] = []
    assert "B1 must map to a gate or artifact" in behavior.validate_manifest(manifest)


def test_real_upstream_surface_adoption_mutation_fails_full_verification(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    source_fixture(tmp_path)
    manifest_path = write_manifest(tmp_path, manifest)
    clean, _ = behavior.verify(manifest_path, tmp_path)
    assert clean == []
    (tmp_path / "surface.txt").write_text(
        "https://github.com/openai/codex/releases/latest"
    )
    errors, _ = behavior.verify(manifest_path, tmp_path)
    assert any("forbidden upstream adoption target" in error for error in errors)


def test_recorded_upstream_base_must_be_fork_head_ancestor(tmp_path: Path) -> None:
    subprocess.run(["git", "init", "-q"], cwd=tmp_path, check=True)
    subprocess.run(["git", "config", "user.name", "Fixture"], cwd=tmp_path, check=True)
    subprocess.run(
        ["git", "config", "user.email", "fixture@example.com"], cwd=tmp_path, check=True
    )
    source = tmp_path / "surface"
    source.write_text("base")
    subprocess.run(["git", "add", "surface"], cwd=tmp_path, check=True)
    subprocess.run(["git", "commit", "-qm", "base"], cwd=tmp_path, check=True)
    base = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=tmp_path, text=True
    ).strip()
    source.write_text("fork")
    subprocess.run(["git", "commit", "-qam", "fork"], cwd=tmp_path, check=True)
    assert behavior.verify_git_ancestry({"upstreamBase": base}, tmp_path) == []
    unrelated = tmp_path / "unrelated"
    subprocess.run(
        ["git", "checkout", "--orphan", "unrelated"], cwd=tmp_path, check=True
    )
    source.unlink()
    unrelated.write_text("unrelated")
    subprocess.run(["git", "add", "-A"], cwd=tmp_path, check=True)
    subprocess.run(["git", "commit", "-qm", "unrelated"], cwd=tmp_path, check=True)
    assert any(
        "is not an ancestor" in error
        for error in behavior.verify_git_ancestry({"upstreamBase": base}, tmp_path)
    )


def test_split_package_layout_and_embedded_provenance_pass(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    errors, metadata = behavior.verify_artifacts(manifest, artifact_fixture(tmp_path))
    assert errors == []
    assert [item["variant"] for item in metadata] == ["codex", "codex-app-server"]


def test_missing_required_helper_or_checksum_fails(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    artifact_root = artifact_fixture(tmp_path)
    (artifact_root / "cli/bin/codex-code-mode-host").unlink()
    errors, _ = behavior.verify_artifacts(manifest, artifact_root)
    assert any(
        "checksum manifest does not exactly cover payloads" in error for error in errors
    )
    assert any("package payload" in error for error in errors)


def test_linux_package_rejects_windows_entrypoint_suffix(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    artifact_root = tmp_path / "artifacts"
    write_package(artifact_root / "cli", "bin/moedex.exe")
    write_package(artifact_root / "app-server", "bin/codex-app-server.exe")
    errors, _ = behavior.verify_artifacts(manifest, artifact_root)
    assert any("expected 'bin/moedex'" in error for error in errors)
    assert any("expected 'bin/codex-app-server'" in error for error in errors)


def test_app_server_package_requires_platform_helpers(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    artifact_root = artifact_fixture(tmp_path)
    app_bwrap = artifact_root / "app-server/codex-resources/bwrap"
    app_bwrap.unlink()
    metadata_path = artifact_root / "app-server/codex-package.json"
    metadata = json.loads(metadata_path.read_text())
    del metadata["checksums"]["codex-resources/bwrap"]
    metadata_path.write_text(json.dumps(metadata))
    errors, _ = behavior.verify_artifacts(manifest, artifact_root)
    assert (
        "app-server: missing required package helper: codex-resources/bwrap" in errors
    )


@pytest.mark.parametrize("variant", ["cli", "app-server"])
def test_macos_package_requires_zsh_after_checksum_refresh(
    tmp_path: Path, variant: str
) -> None:
    manifest = fixture_manifest()
    artifact_root = tmp_path / "artifacts"
    target = "aarch64-apple-darwin"
    write_package(artifact_root / "cli", "bin/moedex", target)
    write_package(artifact_root / "app-server", "bin/codex-app-server", target)
    package = artifact_root / variant
    (package / "codex-resources/zsh/bin/zsh").unlink()
    metadata_path = package / "codex-package.json"
    metadata = json.loads(metadata_path.read_text())
    del metadata["checksums"]["codex-resources/zsh/bin/zsh"]
    metadata_path.write_text(json.dumps(metadata))

    errors, _ = behavior.verify_artifacts(manifest, artifact_root)

    assert any(
        f"{variant}: missing required package helper: codex-resources/zsh/bin/zsh"
        in error
        for error in errors
    )


def test_primary_macos_package_requires_voice_after_checksum_refresh(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    artifact_root = tmp_path / "artifacts"
    target = "x86_64-apple-darwin"
    write_package(artifact_root / "cli", "bin/moedex", target)
    write_package(artifact_root / "app-server", "bin/codex-app-server", target)
    package = artifact_root / "cli"
    voice_files = list((package / "codex-resources/voice").rglob("*"))
    for path in reversed(voice_files):
        if path.is_file():
            path.unlink()
        elif path.is_dir():
            path.rmdir()
    (package / "codex-resources/voice").rmdir()
    metadata_path = package / "codex-package.json"
    metadata = json.loads(metadata_path.read_text())
    metadata["checksums"] = {
        relative: digest
        for relative, digest in metadata["checksums"].items()
        if not relative.startswith("codex-resources/voice/")
    }
    metadata_path.write_text(json.dumps(metadata))

    errors, _ = behavior.verify_artifacts(manifest, artifact_root)

    assert any("cli: missing required voice resource" in error for error in errors)


def test_primary_macos_package_validates_runtime_declared_libraries(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    artifact_root = tmp_path / "artifacts"
    target = "aarch64-apple-darwin"
    write_package(artifact_root / "cli", "bin/moedex", target)
    write_package(artifact_root / "app-server", "bin/codex-app-server", target)
    package = artifact_root / "cli"
    missing = "codex-resources/voice/lib/libgio-2.0.0.dylib"
    (package / missing).unlink()
    package_manifest_path = package / "codex-package.json"
    package_manifest = json.loads(package_manifest_path.read_text())
    del package_manifest["checksums"][missing]
    package_manifest_path.write_text(json.dumps(package_manifest))
    voice_manifest_path = package / "codex-resources/voice/manifest.json"
    voice_manifest = json.loads(voice_manifest_path.read_text())
    del voice_manifest["sha256"][missing]
    voice_manifest_path.write_text(json.dumps(voice_manifest))
    package_manifest["checksums"]["codex-resources/voice/manifest.json"] = (
        hashlib.sha256(voice_manifest_path.read_bytes()).hexdigest()
    )
    package_manifest_path.write_text(json.dumps(package_manifest))

    errors, _ = behavior.verify_artifacts(manifest, artifact_root)

    assert any("invalid voice runtime" in error for error in errors)


def test_primary_macos_package_rejects_malformed_runtime_receipt(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    artifact_root = tmp_path / "artifacts"
    target = "x86_64-apple-darwin"
    write_package(artifact_root / "cli", "bin/moedex", target)
    write_package(artifact_root / "app-server", "bin/codex-app-server", target)
    package = artifact_root / "cli"
    runtime_path = package / "codex-resources/voice/runtime.json"
    runtime_path.write_text("{", encoding="utf-8")
    voice_manifest_path = package / "codex-resources/voice/manifest.json"
    voice_manifest = json.loads(voice_manifest_path.read_text())
    voice_manifest["sha256"]["codex-resources/voice/runtime.json"] = hashlib.sha256(
        runtime_path.read_bytes()
    ).hexdigest()
    voice_manifest_path.write_text(json.dumps(voice_manifest))
    package_manifest_path = package / "codex-package.json"
    package_manifest = json.loads(package_manifest_path.read_text())
    for relative in (
        "codex-resources/voice/runtime.json",
        "codex-resources/voice/manifest.json",
    ):
        package_manifest["checksums"][relative] = hashlib.sha256(
            (package / relative).read_bytes()
        ).hexdigest()
    package_manifest_path.write_text(json.dumps(package_manifest))

    errors, _ = behavior.verify_artifacts(manifest, artifact_root)

    assert any("invalid voice runtime" in error for error in errors)


def test_app_server_macos_package_rejects_unexpected_voice_resources(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    artifact_root = tmp_path / "artifacts"
    target = "aarch64-apple-darwin"
    write_package(artifact_root / "cli", "bin/moedex", target)
    write_package(artifact_root / "app-server", "bin/codex-app-server", target)
    package = artifact_root / "app-server"
    unexpected = package / "codex-resources/voice/manifest.json"
    unexpected.parent.mkdir(parents=True)
    unexpected.write_text("{}", encoding="utf-8")
    metadata_path = package / "codex-package.json"
    metadata = json.loads(metadata_path.read_text())
    metadata["checksums"]["codex-resources/voice/manifest.json"] = hashlib.sha256(
        unexpected.read_bytes()
    ).hexdigest()
    metadata_path.write_text(json.dumps(metadata))

    errors, _ = behavior.verify_artifacts(manifest, artifact_root)

    assert any(
        "app-server: app-server macOS resource inventory mismatch" in error
        for error in errors
    )


def test_package_rejects_unexpected_platform_helper_even_when_checksummed(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    artifact_root = artifact_fixture(tmp_path)
    extra = artifact_root / "app-server/codex-resources/codex-command-runner.exe"
    extra.write_bytes(b"unexpected")
    metadata_path = artifact_root / "app-server/codex-package.json"
    metadata = json.loads(metadata_path.read_text())
    metadata["checksums"]["codex-resources/codex-command-runner.exe"] = hashlib.sha256(
        extra.read_bytes()
    ).hexdigest()
    metadata_path.write_text(json.dumps(metadata))
    errors, _ = behavior.verify_artifacts(manifest, artifact_root)
    assert any("platform helper inventory mismatch" in error for error in errors)


def test_symlink_escape_and_non_regular_artifacts_fail(tmp_path: Path) -> None:
    if os.name == "nt":
        pytest.skip("symlink creation is not generally available")
    manifest = fixture_manifest()
    artifact_root = artifact_fixture(tmp_path)
    outside = tmp_path / "outside"
    outside.write_text("outside")
    entrypoint = artifact_root / "cli/bin/moedex"
    entrypoint.unlink()
    entrypoint.symlink_to(outside)
    errors, _ = behavior.verify_artifacts(manifest, artifact_root)
    assert any(
        "escapes root" in error or "not a regular file" in error for error in errors
    )


def test_unix_executable_bit_is_required(tmp_path: Path) -> None:
    if os.name == "nt":
        pytest.skip("Unix executable mode does not apply")
    manifest = fixture_manifest()
    artifact_root = artifact_fixture(tmp_path)
    entrypoint = artifact_root / "cli/bin/moedex"
    entrypoint.chmod(stat.S_IRUSR | stat.S_IWUSR)
    errors, _ = behavior.verify_artifacts(manifest, artifact_root)
    assert "artifact is not executable: cli/bin/moedex" in errors


def test_evidence_binds_manifest_archives_target_and_provenance(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    manifest_path = write_manifest(tmp_path, manifest)
    _, metadata = behavior.verify_artifacts(manifest, artifact_fixture(tmp_path))
    archives = {
        "cliPackage": tmp_path / "codex-package-x86_64-unknown-linux-musl.tar.gz",
        "appServerPackage": tmp_path
        / "codex-app-server-package-x86_64-unknown-linux-musl.tar.gz",
        "cliSymbols": write_symbols(
            tmp_path / "codex-symbols-x86_64-unknown-linux-musl.tar.gz",
            "codex-symbols-x86_64-unknown-linux-musl",
            ["moedex", "codex-code-mode-host", "codex-responses-api-proxy"],
            "debug",
        ),
        "appServerSymbols": write_symbols(
            tmp_path / "codex-symbols-x86_64-unknown-linux-musl-app-server.tar.gz",
            "codex-symbols-x86_64-unknown-linux-musl-app-server",
            ["codex-app-server", "codex-code-mode-host"],
            "debug",
        ),
    }
    for archive in (archives["cliPackage"], archives["appServerPackage"]):
        archive.write_bytes(archive.name.encode())
    output = tmp_path / "evidence.json"
    behavior.write_evidence(
        manifest_path,
        output,
        "x86_64-unknown-linux-musl",
        "github",
        "f" * 40,
        archives,
        metadata,
        manifest,
        ["package-journeys", "behavior-artifacts"],
    )
    evidence = json.loads(output.read_text())
    assert evidence["upstreamBase"] == UPSTREAM_BASE
    assert evidence["targetTriple"] == "x86_64-unknown-linux-musl"
    assert (
        evidence["manifestSha256"]
        == hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    )
    assert set(evidence["archives"]) == set(archives)
    assert evidence["passedGateIds"] == [
        "behavior-artifacts",
        "package-journeys",
    ]


def test_repository_source_inventory_passes() -> None:
    manifest = repository_manifest()
    assert (
        behavior.verify_source_inventory(manifest, Path(__file__).resolve().parents[1])
        == []
    )


def test_release_evidence_rejects_wrong_target_or_duplicate_symbol_roles(
    tmp_path: Path,
) -> None:
    manifest = fixture_manifest()
    manifest_path = write_manifest(tmp_path, manifest)
    _, metadata = behavior.verify_artifacts(manifest, artifact_fixture(tmp_path))
    cli_symbols = write_symbols(
        tmp_path / "codex-symbols-other-target.tar.gz",
        "codex-symbols-other-target",
        ["moedex", "codex-code-mode-host", "codex-responses-api-proxy"],
        "debug",
    )
    package_paths = {
        "cliPackage": tmp_path / "codex-package-x86_64-unknown-linux-musl.tar.gz",
        "appServerPackage": tmp_path
        / "codex-app-server-package-x86_64-unknown-linux-musl.tar.gz",
    }
    for path in package_paths.values():
        path.write_bytes(path.name.encode())
    archives = {
        **package_paths,
        "cliSymbols": cli_symbols,
        "appServerSymbols": cli_symbols,
    }
    with pytest.raises(ValueError, match="archive roles must reference unique paths"):
        behavior.write_evidence(
            manifest_path,
            tmp_path / "evidence.json",
            "x86_64-unknown-linux-musl",
            "github",
            "f" * 40,
            archives,
            metadata,
            manifest,
            sorted(behavior.RELEASE_EVIDENCE_GATES),
        )

    correct_cli = write_symbols(
        tmp_path / "codex-symbols-x86_64-unknown-linux-musl.tar.gz",
        "codex-symbols-x86_64-unknown-linux-musl",
        ["moedex", "codex-code-mode-host", "codex-responses-api-proxy"],
        "debug",
    )
    wrong_app = write_symbols(
        tmp_path / "codex-symbols-other-app-server.tar.gz",
        "codex-symbols-other-app-server",
        ["codex-app-server", "codex-code-mode-host"],
        "debug",
    )
    archives = {
        **package_paths,
        "cliSymbols": correct_cli,
        "appServerSymbols": wrong_app,
    }
    with pytest.raises(ValueError, match="appServerSymbols archive name must be"):
        behavior.write_evidence(
            manifest_path,
            tmp_path / "evidence.json",
            "x86_64-unknown-linux-musl",
            "github",
            "f" * 40,
            archives,
            metadata,
            manifest,
            sorted(behavior.RELEASE_EVIDENCE_GATES),
        )


def test_windows_release_evidence_uses_one_combined_symbols_archive(
    tmp_path: Path,
) -> None:
    target = "x86_64-pc-windows-msvc"
    manifest = fixture_manifest()
    manifest_path = write_manifest(tmp_path, manifest)
    artifact_root = tmp_path / "artifacts"
    write_package(artifact_root / "cli", "bin/moedex.exe", target)
    write_package(artifact_root / "app-server", "bin/codex-app-server.exe", target)
    errors, metadata = behavior.verify_artifacts(manifest, artifact_root)
    assert errors == []
    cli_package = tmp_path / f"codex-package-{target}.tar.gz"
    app_package = tmp_path / f"codex-app-server-package-{target}.tar.gz"
    cli_package.write_bytes(b"cli")
    app_package.write_bytes(b"app")
    combined = write_symbols(
        tmp_path / f"codex-symbols-{target}.tar.gz",
        f"codex-symbols-{target}",
        [
            "moedex",
            "codex-app-server",
            "codex-code-mode-host",
            "codex-command-runner",
            "codex-responses-api-proxy",
            "codex-windows-sandbox-setup",
        ],
        "pdb",
    )
    archives = {
        "cliPackage": cli_package,
        "appServerPackage": app_package,
        "combinedSymbols": combined,
    }
    behavior.write_evidence(
        manifest_path,
        tmp_path / "evidence.json",
        target,
        "github",
        "f" * 40,
        archives,
        metadata,
        manifest,
        sorted(behavior.RELEASE_EVIDENCE_GATES),
    )


def test_focused_rust_gates_are_bounded_and_reference_known_tests() -> None:
    manifest = repository_manifest()
    expected = {
        "identity": (
            "codex-rs/build-info/src/build_info_tests.rs",
            "version_leads_with_distribution_and_retains_upstream_provenance",
        ),
        "diagnostics": (
            "codex-rs/cli/src/doctor.rs",
            "compatibility_home_is_explicit_in_human_and_json_reports",
        ),
        "login": (
            "codex-rs/login/src/auth/storage_tests.rs",
            "direct_auth_uses_moedex_service",
        ),
        "exec-server": (
            "codex-rs/exec-server/tests/environment_config.rs",
            "remote_executor_resolves_its_own_moedex_home",
        ),
        "tui": (
            "codex-rs/tui/src/chatwidget/tests.rs",
            "terminal_title_preview_uses_moedex_identity",
        ),
    }
    root = Path(__file__).resolve().parents[1]
    for gate_name, (source_path, required_filter) in expected.items():
        argv = manifest["gates"][gate_name]["argv"]
        assert required_filter in " ".join(argv)
        if gate_name == "login":
            assert (
                "importing_file_auth_writes_only_the_destination_and_redacts_outcome"
                in " ".join(argv)
            )
        assert required_filter.split("::")[-1] in (root / source_path).read_text()
    assert manifest["gates"]["tui"]["argv"] != [
        "just",
        "test",
        "-p",
        "codex-tui",
        "--lib",
    ]
    assert manifest["gates"]["exec-server"]["argv"] != [
        "just",
        "test",
        "-p",
        "codex-exec-server",
    ]
    assert manifest["gates"]["brand-inventory"]["argv"] == [
        "python3",
        "scripts/codex_package/test_public_brand_inventory.py",
    ]
