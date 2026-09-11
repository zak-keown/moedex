#!/usr/bin/env python3
"""Validate the Moedex behavior contract and retained release evidence."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = REPO_ROOT / "moedex-behavior-manifest.json"
UPSTREAM_BASE = "8e2afc09126c0cea4c282725fe68af43adad73d7"
REQUIRED_IDS = tuple([f"B{number}" for number in range(1, 14)] + ["U4", "U5"])
STATUSES = {"preserved", "superseded", "intentional-change", "broken"}
RELEASE_EVIDENCE_GATES = {
    "behavior-artifacts",
    "release-package-smoke",
    "release-symbol-smoke",
}
ROOT_FIELDS = {
    "schemaVersion",
    "product",
    "upstreamBase",
    "gates",
    "requirements",
    "sourceInventory",
}
GATE_FIELDS = {"cwd", "argv"}
REQUIREMENT_FIELDS = {
    "id",
    "requirement",
    "status",
    "upstreamBase",
    "tests",
    "artifacts",
    "exclusions",
    "reason",
}
ARTIFACT_FIELDS = {"paths", "executable"}
SOURCE_INVENTORY_FIELDS = {"files", "expectations", "forbiddenRegex"}
EXPECTATION_FIELDS = {"literal", "count"}


def _unknown_fields(value: dict[str, Any], allowed: set[str], label: str) -> list[str]:
    return [f"unknown field in {label}: {field}" for field in sorted(set(value) - allowed)]


def _safe_relative_path(value: object, label: str) -> tuple[str | None, list[str]]:
    if not isinstance(value, str) or not value:
        return None, [f"{label} must be a nonempty relative POSIX path"]
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or "." in path.parts or value != path.as_posix():
        return None, [f"unsafe {label}: {value}"]
    return value, []


def load_manifest(path: Path = MANIFEST_PATH) -> dict[str, Any]:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        value: dict[str, Any] = {}
        for key, item in pairs:
            if key in value:
                raise ValueError(f"duplicate JSON field: {key}")
            value[key] = item
        return value

    with path.open(encoding="utf-8") as manifest_file:
        value = json.load(manifest_file, object_pairs_hook=reject_duplicates)
    if not isinstance(value, dict):
        raise ValueError("manifest root must be an object")
    return value


def validate_manifest(manifest: dict[str, Any]) -> list[str]:
    errors = _unknown_fields(manifest, ROOT_FIELDS, "manifest")
    if manifest.get("schemaVersion") != 1:
        errors.append("schemaVersion must be 1")
    if manifest.get("product") != "Moedex":
        errors.append("product must be Moedex")
    if manifest.get("upstreamBase") != UPSTREAM_BASE:
        errors.append(f"upstreamBase must be {UPSTREAM_BASE}")

    gates = manifest.get("gates")
    if not isinstance(gates, dict):
        errors.append("gates must be an object")
        gates = {}
    seen_gate_names: set[str] = set()
    for name, gate in gates.items():
        if not isinstance(name, str) or not name or name in seen_gate_names:
            errors.append(f"invalid or duplicate gate name: {name!r}")
            continue
        seen_gate_names.add(name)
        if not isinstance(gate, dict):
            errors.append(f"gate {name} must be an object")
            continue
        errors.extend(_unknown_fields(gate, GATE_FIELDS, f"gate {name}"))
        cwd, path_errors = _safe_relative_path(gate.get("cwd"), f"gate {name} cwd")
        errors.extend(path_errors)
        argv = gate.get("argv")
        if not isinstance(argv, list) or not argv or any(not isinstance(arg, str) or not arg for arg in argv):
            errors.append(f"gate {name} argv must be a nonempty string array")
        if cwd is None:
            continue

    requirements = manifest.get("requirements")
    if not isinstance(requirements, list):
        errors.append("requirements must be an array")
        requirements = []
    ids: list[str] = []
    artifact_variants: set[str] = set()
    for index, entry in enumerate(requirements):
        label = f"requirement[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue
        errors.extend(_unknown_fields(entry, REQUIREMENT_FIELDS, label))
        requirement_id = entry.get("id")
        if not isinstance(requirement_id, str) or not requirement_id:
            errors.append(f"{label} id must be nonempty")
            continue
        ids.append(requirement_id)
        label = requirement_id
        if not isinstance(entry.get("requirement"), str) or not entry["requirement"].strip():
            errors.append(f"{label} requirement must be nonempty")
        status = entry.get("status")
        if status not in STATUSES:
            errors.append(f"{label} has unknown status: {status!r}")
        if entry.get("upstreamBase") != manifest.get("upstreamBase"):
            errors.append(f"{label} upstreamBase does not match manifest")
        tests = entry.get("tests")
        if not isinstance(tests, list) or any(not isinstance(test, str) or not test for test in tests):
            errors.append(f"{label} tests must be a string array")
            tests = []
        for test in tests:
            if test not in gates:
                errors.append(f"{label} references missing gate: {test}")
        exclusions = entry.get("exclusions")
        if not isinstance(exclusions, list) or any(not isinstance(item, str) or not item.strip() for item in exclusions):
            errors.append(f"{label} exclusions must be an explicit string array")
        artifacts = entry.get("artifacts")
        if not isinstance(artifacts, list):
            errors.append(f"{label} artifacts must be an array")
            artifacts = []
        for artifact_index, artifact in enumerate(artifacts):
            artifact_label = f"{label} artifact[{artifact_index}]"
            if not isinstance(artifact, dict):
                errors.append(f"{artifact_label} must be an object")
                continue
            errors.extend(_unknown_fields(artifact, ARTIFACT_FIELDS, artifact_label))
            paths = artifact.get("paths")
            if not isinstance(paths, list) or not paths:
                errors.append(f"{artifact_label} paths must be a nonempty array")
                continue
            for value in paths:
                path, path_errors = _safe_relative_path(value, f"{artifact_label} path")
                errors.extend(path_errors)
                if path is not None:
                    if path in artifact_variants:
                        errors.append(f"duplicate artifact path: {path}")
                    artifact_variants.add(path)
            if not isinstance(artifact.get("executable"), bool):
                errors.append(f"{artifact_label} executable must be boolean")
        if status == "broken":
            if not isinstance(entry.get("reason"), str) or not entry["reason"].strip():
                errors.append(f"broken requirement {label} must have a reason")
        elif not tests and not artifacts:
            errors.append(f"{label} must map to a gate or artifact")

    duplicate_ids = sorted({item for item in ids if ids.count(item) > 1})
    if duplicate_ids:
        errors.append("duplicate requirement ids: " + ", ".join(duplicate_ids))
    missing_ids = sorted(set(REQUIRED_IDS) - set(ids))
    unexpected_ids = sorted(set(ids) - set(REQUIRED_IDS))
    if missing_ids:
        errors.append("missing requirement ids: " + ", ".join(missing_ids))
    if unexpected_ids:
        errors.append("unexpected requirement ids: " + ", ".join(unexpected_ids))

    inventory = manifest.get("sourceInventory")
    if not isinstance(inventory, dict):
        errors.append("sourceInventory must be an object")
    else:
        errors.extend(_unknown_fields(inventory, SOURCE_INVENTORY_FIELDS, "sourceInventory"))
        files = inventory.get("files")
        if not isinstance(files, list) or not files:
            errors.append("sourceInventory files must be a nonempty array")
        else:
            seen_files: set[str] = set()
            for value in files:
                path, path_errors = _safe_relative_path(value, "source inventory file")
                errors.extend(path_errors)
                if path is not None:
                    if path in seen_files:
                        errors.append(f"duplicate source inventory file: {path}")
                    seen_files.add(path)
        expectations = inventory.get("expectations")
        if not isinstance(expectations, list) or not expectations:
            errors.append("sourceInventory expectations must be a nonempty array")
        else:
            seen_literals: set[str] = set()
            for index, expectation in enumerate(expectations):
                label = f"source expectation[{index}]"
                if not isinstance(expectation, dict):
                    errors.append(f"{label} must be an object")
                    continue
                errors.extend(_unknown_fields(expectation, EXPECTATION_FIELDS, label))
                literal = expectation.get("literal")
                count = expectation.get("count")
                if not isinstance(literal, str) or not literal:
                    errors.append(f"{label} literal must be nonempty")
                elif literal in seen_literals:
                    errors.append(f"duplicate source expectation: {literal}")
                else:
                    seen_literals.add(literal)
                if not isinstance(count, int) or isinstance(count, bool) or count < 0:
                    errors.append(f"{label} count must be a nonnegative integer")
        forbidden = inventory.get("forbiddenRegex")
        if not isinstance(forbidden, list) or not forbidden or any(not isinstance(pattern, str) or not pattern for pattern in forbidden):
            errors.append("sourceInventory forbiddenRegex must be a nonempty string array")
        else:
            for pattern in forbidden:
                try:
                    re.compile(pattern)
                except re.error as error:
                    errors.append(f"invalid forbiddenRegex {pattern!r}: {error}")
    return errors


def _resolve_regular_file(root: Path, relative: str, label: str) -> tuple[Path | None, list[str]]:
    candidate = root / relative
    try:
        resolved_root = root.resolve(strict=True)
        resolved = candidate.resolve(strict=True)
    except FileNotFoundError:
        return None, [f"missing {label}: {relative}"]
    if resolved_root != resolved and resolved_root not in resolved.parents:
        return None, [f"{label} escapes root: {relative}"]
    if candidate.is_symlink() or not resolved.is_file():
        return None, [f"{label} is not a regular file: {relative}"]
    return resolved, []


def verify_source_inventory(manifest: dict[str, Any], repo_root: Path) -> list[str]:
    inventory = manifest["sourceInventory"]
    contents: list[str] = []
    errors: list[str] = []
    for relative in inventory["files"]:
        path, path_errors = _resolve_regular_file(repo_root, relative, "source inventory file")
        errors.extend(path_errors)
        if path is not None:
            contents.append(path.read_text(encoding="utf-8"))
    joined = "\n".join(contents)
    for expectation in inventory["expectations"]:
        actual = joined.count(expectation["literal"])
        if actual != expectation["count"]:
            errors.append(
                f"source occurrence mismatch for {expectation['literal']!r}: "
                f"expected {expectation['count']}, got {actual}"
            )
    for pattern in inventory["forbiddenRegex"]:
        if re.search(pattern, joined, re.IGNORECASE):
            errors.append(f"forbidden upstream adoption target matched: {pattern}")
    return errors


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _validate_package(
    package_root: Path, expected_entrypoints: set[str], expected_variant: str
) -> tuple[list[str], dict[str, Any] | None]:
    errors: list[str] = []
    manifest_path, path_errors = _resolve_regular_file(package_root, "codex-package.json", "package manifest")
    errors.extend(path_errors)
    if manifest_path is None:
        return errors, None
    try:
        metadata = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        return [*errors, f"invalid package manifest: {error}"], None
    entrypoint = metadata.get("entrypoint")
    if entrypoint not in expected_entrypoints:
        errors.append(f"unexpected package entrypoint: {entrypoint!r}")
    if metadata.get("layoutVersion") != 1:
        errors.append("package layoutVersion must be 1")
    if metadata.get("variant") != expected_variant:
        errors.append(f"unexpected package variant: {metadata.get('variant')!r}")
    target = metadata.get("target")
    if not isinstance(target, str) or not target:
        errors.append("package target must be nonempty")
        target = ""
    if not isinstance(metadata.get("version"), str) or not metadata["version"]:
        errors.append("package version must be nonempty")
    if metadata.get("pathDir") != "codex-path" or metadata.get("resourcesDir") != "codex-resources":
        errors.append("package helper directories do not match layout version 1")
    provenance = metadata.get("provenance")
    if not isinstance(provenance, dict):
        errors.append("package provenance must be an object")
    else:
        for key, expected in {"product": "moedex", "repository": "zak-keown/moedex"}.items():
            if provenance.get(key) != expected:
                errors.append(f"invalid package provenance {key}: {provenance.get(key)!r}")
        for key in ("forkCommit", "upstreamCommit", "releaseChannel"):
            if not isinstance(provenance.get(key), str) or provenance[key].strip().lower() in {"", "unknown"}:
                errors.append(f"package provenance {key} is missing")
    checksums = metadata.get("checksums")
    if not isinstance(checksums, dict):
        errors.append("package checksums must be an object")
        return errors, metadata
    executable_suffix = ".exe" if "windows" in target else ""
    required_helpers = [
        f"bin/codex-code-mode-host{executable_suffix}",
        f"codex-path/rg{executable_suffix}",
    ]
    if expected_variant == "codex" and "linux" in target:
        required_helpers.append("codex-resources/bwrap")
    if expected_variant == "codex" and "windows" in target:
        required_helpers.extend(
            [
                "codex-resources/codex-command-runner.exe",
                "codex-resources/codex-windows-sandbox-setup.exe",
            ]
        )
    if isinstance(entrypoint, str):
        required_helpers.append(entrypoint)
    for relative in required_helpers:
        helper, helper_errors = _resolve_regular_file(package_root, relative, "required package helper")
        errors.extend(helper_errors)
        if helper is not None and os.name != "nt" and target and "windows" not in target:
            if helper.stat().st_mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH) == 0:
                errors.append(f"required package helper is not executable: {relative}")
    payloads = {
        path.relative_to(package_root).as_posix()
        for path in package_root.rglob("*")
        if path.is_file() and path.name != "codex-package.json"
    }
    if set(checksums) != payloads:
        errors.append("package checksum manifest does not exactly cover payloads")
    for relative, expected in checksums.items():
        safe, safe_errors = _safe_relative_path(relative, "package checksum path")
        errors.extend(safe_errors)
        if safe is None:
            continue
        path, file_errors = _resolve_regular_file(package_root, safe, "package payload")
        errors.extend(file_errors)
        if path is not None and (not isinstance(expected, str) or _sha256(path) != expected):
            errors.append(f"package checksum mismatch: {relative}")
    return errors, metadata


def verify_artifacts(manifest: dict[str, Any], artifact_root: Path) -> tuple[list[str], list[dict[str, Any]]]:
    errors: list[str] = []
    package_metadata: list[dict[str, Any]] = []
    cli_errors, cli = _validate_package(
        artifact_root / "cli", {"bin/moedex", "bin/moedex.exe"}, "codex"
    )
    app_errors, app = _validate_package(
        artifact_root / "app-server",
        {"bin/codex-app-server", "bin/codex-app-server.exe"},
        "codex-app-server",
    )
    errors.extend(f"cli: {error}" for error in cli_errors)
    errors.extend(f"app-server: {error}" for error in app_errors)
    if cli is not None:
        package_metadata.append(cli)
    if app is not None:
        package_metadata.append(app)
    if cli is not None and app is not None:
        for field in ("target", "version", "provenance"):
            if cli.get(field) != app.get(field):
                errors.append(f"CLI and app-server package {field} differ")

    for entry in manifest["requirements"]:
        for artifact in entry["artifacts"]:
            matches: list[tuple[str, Path]] = []
            for relative in artifact["paths"]:
                path, _ = _resolve_regular_file(artifact_root, relative, "artifact")
                if path is not None:
                    matches.append((relative, path))
            if len(matches) != 1:
                errors.append(
                    f"{entry['id']} artifact requires exactly one of: " + ", ".join(artifact["paths"])
                )
                continue
            relative, path = matches[0]
            if artifact["executable"] and os.name != "nt":
                mode = path.stat().st_mode
                if mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH) == 0:
                    errors.append(f"artifact is not executable: {relative}")
    return errors, package_metadata


def verify(manifest_path: Path, repo_root: Path, artifact_dir: Path | None = None) -> tuple[list[str], list[dict[str, Any]]]:
    try:
        manifest = load_manifest(manifest_path)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return [f"could not load manifest: {error}"], []
    errors = validate_manifest(manifest)
    package_metadata: list[dict[str, Any]] = []
    if not errors:
        errors.extend(verify_source_inventory(manifest, repo_root))
        if artifact_dir is not None:
            artifact_errors, package_metadata = verify_artifacts(manifest, artifact_dir)
            errors.extend(artifact_errors)
    if any(entry.get("status") == "broken" for entry in manifest.get("requirements", [])):
        errors.append("manifest contains a broken requirement")
    return errors, package_metadata


def write_evidence(
    manifest_path: Path,
    output: Path,
    target: str,
    channel: str,
    fork_commit: str,
    archives: list[Path],
    package_metadata: list[dict[str, Any]],
    manifest: dict[str, Any],
    passed_gate_ids: list[str],
) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", fork_commit):
        raise ValueError("fork commit must be a full lowercase Git SHA")
    if channel != "github":
        raise ValueError("release evidence channel must be github")
    if set(passed_gate_ids) != RELEASE_EVIDENCE_GATES or len(passed_gate_ids) != len(
        RELEASE_EVIDENCE_GATES
    ):
        raise ValueError("release evidence must record every archive qualification gate exactly once")
    if len(archives) != 4:
        raise ValueError("release evidence requires two package and two symbols archives")
    expected_packages = {
        f"codex-package-{target}.tar.gz",
        f"codex-app-server-package-{target}.tar.gz",
    }
    archive_names = [path.name for path in archives]
    if not expected_packages.issubset(archive_names):
        raise ValueError("release evidence package archive names do not match target")
    if sum(name.startswith("codex-symbols-") and name.endswith(".tar.gz") for name in archive_names) != 2:
        raise ValueError("release evidence requires two symbols archive inputs")
    for archive in archives:
        if archive.is_symlink() or not archive.is_file():
            raise ValueError(f"release evidence archive is not a regular file: {archive}")
    provenance = package_metadata[0]["provenance"]
    evidence = {
        "schemaVersion": 1,
        "product": "Moedex",
        "manifestSha256": _sha256(manifest_path),
        "forkCommit": fork_commit,
        "upstreamBase": manifest["upstreamBase"],
        "releaseChannel": channel,
        "targetTriple": target,
        "archives": [
            {"name": path.name, "sha256": _sha256(path)} for path in archives
        ],
        "embeddedProvenance": provenance,
        "passedGateIds": sorted(set(passed_gate_ids)),
    }
    if provenance.get("forkCommit") != fork_commit:
        raise ValueError("embedded fork commit does not match evidence")
    if provenance.get("upstreamCommit") != manifest["upstreamBase"]:
        raise ValueError("embedded upstream commit does not match behavior manifest")
    if provenance.get("releaseChannel") != channel:
        raise ValueError("embedded release channel does not match evidence")
    if any(metadata.get("target") != target for metadata in package_metadata):
        raise ValueError("package target does not match evidence")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=MANIFEST_PATH)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    subparsers = parser.add_subparsers(dest="command", required=True)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--artifact-dir", type=Path)
    evidence_parser = subparsers.add_parser("evidence")
    evidence_parser.add_argument("--artifact-dir", type=Path, required=True)
    evidence_parser.add_argument("--target", required=True)
    evidence_parser.add_argument("--channel", required=True)
    evidence_parser.add_argument("--fork-commit", required=True)
    evidence_parser.add_argument("--archive", type=Path, action="append", required=True)
    evidence_parser.add_argument("--passed-gate", action="append", required=True)
    evidence_parser.add_argument("--output", type=Path, required=True)
    gates_parser = subparsers.add_parser("run-gates")
    gates_parser.add_argument("--gate", action="append", required=True)
    args = parser.parse_args(argv)

    errors, package_metadata = verify(args.manifest, args.repo_root, getattr(args, "artifact_dir", None))
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1
    manifest = load_manifest(args.manifest)
    if args.command == "run-gates":
        unknown = sorted(set(args.gate) - set(manifest["gates"]))
        if unknown:
            print(f"error: unknown gates: {', '.join(unknown)}", file=sys.stderr)
            return 1
        for name in args.gate:
            gate = manifest["gates"][name]
            print(f"running gate {name}: {' '.join(gate['argv'])}", flush=True)
            result = subprocess.run(
                gate["argv"], cwd=args.repo_root / gate["cwd"], check=False
            )
            if result.returncode:
                print(f"error: gate {name} failed with {result.returncode}", file=sys.stderr)
                return result.returncode
    if args.command == "evidence":
        try:
            write_evidence(
                args.manifest,
                args.output,
                args.target,
                args.channel,
                args.fork_commit,
                args.archive,
                package_metadata,
                manifest,
                args.passed_gate,
            )
        except (OSError, ValueError) as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
    print(f"verified {len(manifest['requirements'])} Moedex behavior requirements")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
