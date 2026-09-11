#!/usr/bin/env python3

import copy
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import moedex_behavior_manifest as behavior


UPSTREAM_BASE = "8e2afc09126c0cea4c282725fe68af43adad73d7"


def repository_manifest() -> dict:
    return behavior.load_manifest(
        Path(__file__).resolve().parents[1] / "moedex-behavior-manifest.json"
    )


def fixture_manifest() -> dict:
    manifest = copy.deepcopy(repository_manifest())
    manifest["sourceInventory"] = {
        "files": ["surface.txt"],
        "expectations": [{"literal": "github.com/zak-keown/moedex", "count": 1}],
        "forbiddenRegex": [r"github\.com/openai/codex/releases"],
    }
    return manifest


def write_manifest(root: Path, manifest: dict) -> Path:
    path = root / "manifest.json"
    path.write_text(json.dumps(manifest), encoding="utf-8")
    return path


def write_package(root: Path, entrypoint: str, target: str = "x86_64-unknown-linux-musl") -> None:
    payloads = [entrypoint, "bin/codex-code-mode-host", "codex-path/rg"]
    if entrypoint.endswith("moedex"):
        payloads.append("codex-resources/bwrap")
    for relative in payloads:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(relative, encoding="utf-8")
        path.chmod(0o755)
    metadata = {
        "layoutVersion": 1,
        "version": "0.1.0",
        "target": target,
        "variant": "codex" if entrypoint.endswith("moedex") else "codex-app-server",
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


def test_repository_manifest_is_complete_and_strict() -> None:
    manifest = repository_manifest()
    assert behavior.validate_manifest(manifest) == []
    assert [entry["id"] for entry in manifest["requirements"]] == list(
        behavior.REQUIRED_IDS
    )
    assert all("exclusions" in entry for entry in manifest["requirements"])


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


def test_broken_requirement_never_certifies(tmp_path: Path) -> None:
    manifest = fixture_manifest()
    entry = manifest["requirements"][0]
    entry["status"] = "broken"
    entry["reason"] = "fixture breakage"
    (tmp_path / "surface.txt").write_text("github.com/zak-keown/moedex")
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
    source = tmp_path / "surface.txt"
    source.write_text("github.com/zak-keown/moedex")
    assert behavior.verify_source_inventory(manifest, tmp_path) == []

    source.write_text("github.com/zak-keown/moedex\ngithub.com/zak-keown/moedex")
    assert any(
        "source occurrence mismatch" in error
        for error in behavior.verify_source_inventory(manifest, tmp_path)
    )

    source.write_text(
        "github.com/zak-keown/moedex\nhttps://github.com/openai/codex/releases/latest"
    )
    assert any(
        "forbidden upstream adoption target" in error
        for error in behavior.verify_source_inventory(manifest, tmp_path)
    )


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
    assert any("checksum manifest does not exactly cover payloads" in error for error in errors)
    assert any("package payload" in error for error in errors)


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
    assert any("escapes root" in error or "not a regular file" in error for error in errors)


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
    archives = [
        tmp_path / "codex-package-x86_64-unknown-linux-musl.tar.gz",
        tmp_path / "codex-app-server-package-x86_64-unknown-linux-musl.tar.gz",
        tmp_path / "codex-symbols-x86_64-unknown-linux-musl.tar.gz",
        tmp_path / "codex-symbols-x86_64-unknown-linux-musl-app-server.tar.gz",
    ]
    for archive in archives:
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
        ["release-package-smoke", "release-symbol-smoke", "behavior-artifacts"],
    )
    evidence = json.loads(output.read_text())
    assert evidence["upstreamBase"] == UPSTREAM_BASE
    assert evidence["targetTriple"] == "x86_64-unknown-linux-musl"
    assert evidence["manifestSha256"] == hashlib.sha256(
        manifest_path.read_bytes()
    ).hexdigest()
    assert {item["name"] for item in evidence["archives"]} == {
        archive.name for archive in archives
    }
    assert evidence["passedGateIds"] == [
        "behavior-artifacts",
        "release-package-smoke",
        "release-symbol-smoke",
    ]


def test_repository_source_inventory_passes() -> None:
    manifest = repository_manifest()
    assert behavior.verify_source_inventory(
        manifest, Path(__file__).resolve().parents[1]
    ) == []
