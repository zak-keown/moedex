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
import tarfile
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
VOICE_RUNTIME_ROOT = REPO_ROOT / "third_party/voice"
sys.path.insert(0, str(VOICE_RUNTIME_ROOT))
from package_runtime import runtime_files as validate_voice_runtime_files

MANIFEST_PATH = REPO_ROOT / "moedex-behavior-manifest.json"
UPSTREAM_BASE = "8e2afc09126c0cea4c282725fe68af43adad73d7"
REQUIRED_IDS = tuple([f"B{number}" for number in range(1, 14)] + ["U4", "U5"])
STATUSES = {"preserved", "superseded", "intentional-change", "broken"}
RELEASE_EVIDENCE_GATES = {
    "behavior-artifacts",
    "package-journeys",
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
SOURCE_INVENTORY_FIELDS = {
    "files",
    "discoveryGlobs",
    "expectations",
    "forbiddenRegex",
    "policyScan",
}
EXPECTATION_FIELDS = {"path", "literal", "count"}
FORBIDDEN_FIELDS = {"path", "pattern"}
POLICY_SCAN_FIELDS = {
    "roots",
    "extensions",
    "excludedDirectories",
    "excludedFileGlobs",
    "forbiddenRegex",
    "allowedOccurrences",
}
POLICY_ALLOWED_FIELDS = {"path", "pattern", "count"}


def _unknown_fields(value: dict[str, Any], allowed: set[str], label: str) -> list[str]:
    return [
        f"unknown field in {label}: {field}" for field in sorted(set(value) - allowed)
    ]


def _safe_relative_path(value: object, label: str) -> tuple[str | None, list[str]]:
    if not isinstance(value, str) or not value:
        return None, [f"{label} must be a nonempty relative POSIX path"]
    path = PurePosixPath(value)
    if (
        path.is_absolute()
        or ".." in path.parts
        or "." in path.parts
        or value != path.as_posix()
    ):
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
        if (
            not isinstance(argv, list)
            or not argv
            or any(not isinstance(arg, str) or not arg for arg in argv)
        ):
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
        if (
            not isinstance(entry.get("requirement"), str)
            or not entry["requirement"].strip()
        ):
            errors.append(f"{label} requirement must be nonempty")
        status = entry.get("status")
        if status not in STATUSES:
            errors.append(f"{label} has unknown status: {status!r}")
        if entry.get("upstreamBase") != manifest.get("upstreamBase"):
            errors.append(f"{label} upstreamBase does not match manifest")
        tests = entry.get("tests")
        if not isinstance(tests, list) or any(
            not isinstance(test, str) or not test for test in tests
        ):
            errors.append(f"{label} tests must be a string array")
            tests = []
        for test in tests:
            if test not in gates:
                errors.append(f"{label} references missing gate: {test}")
        exclusions = entry.get("exclusions")
        if not isinstance(exclusions, list) or any(
            not isinstance(item, str) or not item.strip() for item in exclusions
        ):
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
        if status in {"preserved", "superseded"} and not tests:
            errors.append(f"{label} {status} behavior requires an executable gate")

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
        errors.extend(
            _unknown_fields(inventory, SOURCE_INVENTORY_FIELDS, "sourceInventory")
        )
        seen_files: set[str] = set()
        files = inventory.get("files")
        if not isinstance(files, list) or not files:
            errors.append("sourceInventory files must be a nonempty array")
        else:
            for value in files:
                path, path_errors = _safe_relative_path(value, "source inventory file")
                errors.extend(path_errors)
                if path is not None:
                    if path in seen_files:
                        errors.append(f"duplicate source inventory file: {path}")
                    seen_files.add(path)
        discovery_globs = inventory.get("discoveryGlobs")
        if not isinstance(discovery_globs, list) or not discovery_globs:
            errors.append("sourceInventory discoveryGlobs must be a nonempty array")
        else:
            seen_globs: set[str] = set()
            for value in discovery_globs:
                pattern, pattern_errors = _safe_relative_path(
                    value, "source inventory discovery glob"
                )
                errors.extend(pattern_errors)
                if pattern is not None:
                    if pattern in seen_globs:
                        errors.append(
                            f"duplicate source inventory discovery glob: {pattern}"
                        )
                    seen_globs.add(pattern)
        policy = inventory.get("policyScan")
        if not isinstance(policy, dict):
            errors.append("sourceInventory policyScan must be an object")
        else:
            errors.extend(
                _unknown_fields(
                    policy, POLICY_SCAN_FIELDS, "sourceInventory policyScan"
                )
            )
            for field in ("roots", "excludedFileGlobs"):
                values = policy.get(field)
                if not isinstance(values, list) or not values:
                    errors.append(
                        f"sourceInventory policyScan {field} must be nonempty"
                    )
                    continue
                seen: set[str] = set()
                for value in values:
                    path, path_errors = _safe_relative_path(
                        value, f"sourceInventory policyScan {field} entry"
                    )
                    errors.extend(path_errors)
                    if path is not None:
                        if path in seen:
                            errors.append(
                                f"duplicate sourceInventory policyScan {field}: {path}"
                            )
                        seen.add(path)
            extensions = policy.get("extensions")
            if (
                not isinstance(extensions, list)
                or not extensions
                or any(
                    not isinstance(value, str)
                    or (value and not value.startswith("."))
                    or "/" in value
                    for value in extensions
                )
            ):
                errors.append(
                    "sourceInventory policyScan extensions must be file suffixes"
                )
            excluded = policy.get("excludedDirectories")
            if (
                not isinstance(excluded, list)
                or not excluded
                or any(
                    not isinstance(value, str)
                    or not value
                    or "/" in value
                    or value in {".", ".."}
                    for value in excluded
                )
            ):
                errors.append(
                    "sourceInventory policyScan excludedDirectories must be names"
                )
            policy_patterns = policy.get("forbiddenRegex")
            if not isinstance(policy_patterns, list) or not policy_patterns:
                errors.append(
                    "sourceInventory policyScan forbiddenRegex must be nonempty"
                )
            else:
                for pattern in policy_patterns:
                    if not isinstance(pattern, str) or not pattern:
                        errors.append("policy scan forbiddenRegex must be nonempty")
                        continue
                    try:
                        re.compile(pattern)
                    except re.error as error:
                        errors.append(f"invalid policy scan regex {pattern!r}: {error}")
            allowed = policy.get("allowedOccurrences")
            if not isinstance(allowed, list):
                errors.append(
                    "sourceInventory policyScan allowedOccurrences must be an array"
                )
            else:
                for index, item in enumerate(allowed):
                    label = f"policy allowedOccurrences[{index}]"
                    if not isinstance(item, dict):
                        errors.append(f"{label} must be an object")
                        continue
                    errors.extend(_unknown_fields(item, POLICY_ALLOWED_FIELDS, label))
                    _, path_errors = _safe_relative_path(
                        item.get("path"), f"{label} path"
                    )
                    errors.extend(path_errors)
                    if item.get("pattern") not in (policy_patterns or []):
                        errors.append(f"{label} pattern is not a policy regex")
                    count = item.get("count")
                    if (
                        not isinstance(count, int)
                        or isinstance(count, bool)
                        or count < 0
                    ):
                        errors.append(f"{label} count must be a nonnegative integer")
        covered_files: set[str] = set()
        expectations = inventory.get("expectations")
        if not isinstance(expectations, list) or not expectations:
            errors.append("sourceInventory expectations must be a nonempty array")
        else:
            seen_expectations: set[tuple[str, str]] = set()
            for index, expectation in enumerate(expectations):
                label = f"source expectation[{index}]"
                if not isinstance(expectation, dict):
                    errors.append(f"{label} must be an object")
                    continue
                errors.extend(_unknown_fields(expectation, EXPECTATION_FIELDS, label))
                path, path_errors = _safe_relative_path(
                    expectation.get("path"), f"{label} path"
                )
                errors.extend(path_errors)
                if path is not None and path not in seen_files:
                    errors.append(f"{label} path is not inventoried: {path}")
                elif path is not None:
                    covered_files.add(path)
                literal = expectation.get("literal")
                count = expectation.get("count")
                if not isinstance(literal, str) or not literal:
                    errors.append(f"{label} literal must be nonempty")
                elif (expectation.get("path"), literal) in seen_expectations:
                    errors.append(
                        f"duplicate source expectation: {expectation.get('path')} {literal}"
                    )
                else:
                    seen_expectations.add((expectation.get("path"), literal))
                if not isinstance(count, int) or isinstance(count, bool) or count < 0:
                    errors.append(f"{label} count must be a nonnegative integer")
        forbidden = inventory.get("forbiddenRegex")
        if not isinstance(forbidden, list) or not forbidden:
            errors.append("sourceInventory forbiddenRegex must be a nonempty array")
        else:
            for index, item in enumerate(forbidden):
                label = f"source forbiddenRegex[{index}]"
                if not isinstance(item, dict):
                    errors.append(f"{label} must be an object")
                    continue
                errors.extend(_unknown_fields(item, FORBIDDEN_FIELDS, label))
                path, path_errors = _safe_relative_path(
                    item.get("path"), f"{label} path"
                )
                errors.extend(path_errors)
                if path is not None and path not in seen_files:
                    errors.append(f"{label} path is not inventoried: {path}")
                elif path is not None:
                    covered_files.add(path)
                pattern = item.get("pattern")
                if not isinstance(pattern, str) or not pattern:
                    errors.append(f"{label} pattern must be nonempty")
                    continue
                try:
                    re.compile(pattern)
                except re.error as error:
                    errors.append(f"invalid forbiddenRegex {pattern!r}: {error}")
        uncovered = sorted(seen_files - covered_files)
        if uncovered:
            errors.append(
                "source inventory files without checks: " + ", ".join(uncovered)
            )
    return errors


def _resolve_regular_file(
    root: Path, relative: str, label: str
) -> tuple[Path | None, list[str]]:
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
    contents: dict[str, str] = {}
    errors: list[str] = []
    discovered = {
        path.relative_to(repo_root).as_posix()
        for pattern in inventory["discoveryGlobs"]
        for path in repo_root.glob(pattern)
        if path.is_file() or path.is_symlink()
    }
    inventoried = set(inventory["files"])
    if discovered != inventoried:
        errors.append(
            "discovered source inventory differs from manifest: "
            f"missing {sorted(discovered - inventoried)}, "
            f"unexpected {sorted(inventoried - discovered)}"
        )
    for relative in inventory["files"]:
        path, path_errors = _resolve_regular_file(
            repo_root, relative, "source inventory file"
        )
        errors.extend(path_errors)
        if path is not None:
            contents[relative] = path.read_text(encoding="utf-8")
    for expectation in inventory["expectations"]:
        actual = contents.get(expectation["path"], "").count(expectation["literal"])
        if actual != expectation["count"]:
            errors.append(
                f"source occurrence mismatch for {expectation['literal']!r}: "
                f"expected {expectation['count']}, got {actual}"
            )
    for item in inventory["forbiddenRegex"]:
        if re.search(item["pattern"], contents.get(item["path"], ""), re.IGNORECASE):
            errors.append(
                "forbidden upstream adoption target matched in "
                f"{item['path']}: {item['pattern']}"
            )
    policy = inventory["policyScan"]
    excluded_directories = set(policy["excludedDirectories"])
    excluded_globs = tuple(policy["excludedFileGlobs"])
    extensions = set(policy["extensions"])
    policy_contents: dict[str, str] = {}
    for relative_root in policy["roots"]:
        root = repo_root / relative_root
        try:
            resolved_root = root.resolve(strict=True)
            resolved_repo = repo_root.resolve(strict=True)
        except FileNotFoundError:
            errors.append(f"missing policy scan root: {relative_root}")
            continue
        if (
            resolved_repo != resolved_root
            and resolved_repo not in resolved_root.parents
        ):
            errors.append(f"policy scan root escapes repository: {relative_root}")
            continue
        for path in root.rglob("*"):
            relative = path.relative_to(repo_root)
            if (
                any(part in excluded_directories for part in relative.parts)
                or any(relative.match(pattern) for pattern in excluded_globs)
                or path.suffix not in extensions
            ):
                continue
            if path.is_symlink():
                errors.append(f"policy scan file is a symlink: {relative.as_posix()}")
                continue
            try:
                resolved = path.resolve(strict=True)
            except FileNotFoundError:
                errors.append(f"missing policy scan file: {relative.as_posix()}")
                continue
            if resolved_repo != resolved and resolved_repo not in resolved.parents:
                errors.append(
                    f"policy scan file escapes repository: {relative.as_posix()}"
                )
                continue
            if not resolved.is_file():
                continue
            try:
                policy_contents[relative.as_posix()] = resolved.read_text(
                    encoding="utf-8"
                )
            except UnicodeDecodeError:
                continue
    allowed = {
        (item["path"], item["pattern"]): item["count"]
        for item in policy["allowedOccurrences"]
    }
    for path, contents_text in policy_contents.items():
        for pattern in policy["forbiddenRegex"]:
            count = len(re.findall(pattern, contents_text, re.IGNORECASE))
            expected = allowed.get((path, pattern), 0)
            if count != expected:
                errors.append(
                    f"policy scan forbidden target occurrence mismatch in {path}: "
                    f"expected {expected}, got {count} for {pattern}"
                )
    return errors


def verify_git_ancestry(
    manifest: dict[str, Any], repo_root: Path, fork_commit: str = "HEAD"
) -> list[str]:
    base = manifest["upstreamBase"]
    try:
        result = subprocess.run(
            ["git", "merge-base", "--is-ancestor", base, fork_commit],
            cwd=repo_root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except OSError as error:
        return [f"could not verify upstream ancestry: {error}"]
    if result.returncode != 0:
        detail = result.stderr.strip()
        return [
            f"recorded upstreamBase {base} is not an ancestor of {fork_commit}"
            + (f": {detail}" if detail else "")
        ]
    merge_base = subprocess.run(
        ["git", "merge-base", base, fork_commit],
        cwd=repo_root,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if merge_base.returncode != 0 or merge_base.stdout.strip() != base:
        return [f"fork merge-base does not equal recorded upstreamBase {base}"]
    return []


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _symbol_names(path: Path, expected_root: str, target: str) -> set[str]:
    try:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
    except tarfile.TarError as error:
        raise ValueError(f"invalid symbols archive {path.name}: {error}") from error
    names: set[str] = set()
    for member in members:
        relative = PurePosixPath(member.name)
        if relative.is_absolute() or ".." in relative.parts or not relative.parts:
            raise ValueError(f"unsafe symbols archive member: {member.name}")
        if relative.parts[0] != expected_root:
            raise ValueError(
                f"symbols archive {path.name} has unexpected root {relative.parts[0]!r}"
            )
        if member.issym() or member.islnk():
            raise ValueError(f"symbols archive contains link: {member.name}")
        if len(relative.parts) < 2:
            continue
        item = relative.parts[1]
        if "windows" in target and member.isfile() and item.endswith(".pdb"):
            names.add(item.removesuffix(".pdb"))
        elif "linux" in target and member.isfile() and item.endswith(".debug"):
            names.add(item.removesuffix(".debug"))
        elif "apple-darwin" in target and item.endswith(".dSYM"):
            names.add(item.removesuffix(".dSYM"))
    return names


def _validate_release_archives(target: str, archives: dict[str, Path]) -> None:
    windows = "windows" in target
    expected_roles = (
        {"cliPackage", "appServerPackage", "combinedSymbols"}
        if windows
        else {"cliPackage", "appServerPackage", "cliSymbols", "appServerSymbols"}
    )
    if set(archives) != expected_roles:
        raise ValueError(
            "release archive roles must be exactly: "
            + ", ".join(sorted(expected_roles))
        )
    resolved_paths = [path.resolve() for path in archives.values()]
    if len(set(resolved_paths)) != len(resolved_paths):
        raise ValueError("archive roles must reference unique paths")
    expected_names = {
        "cliPackage": f"codex-package-{target}.tar.gz",
        "appServerPackage": f"codex-app-server-package-{target}.tar.gz",
    }
    if windows:
        expected_names["combinedSymbols"] = f"codex-symbols-{target}.tar.gz"
    else:
        expected_names.update(
            {
                "cliSymbols": f"codex-symbols-{target}.tar.gz",
                "appServerSymbols": f"codex-symbols-{target}-app-server.tar.gz",
            }
        )
    for role, expected_name in expected_names.items():
        path = archives[role]
        if path.name != expected_name:
            raise ValueError(
                f"{role} archive name must be {expected_name}, got {path.name}"
            )
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"release evidence archive is not a regular file: {path}")
    if windows:
        actual = _symbol_names(
            archives["combinedSymbols"], f"codex-symbols-{target}", target
        )
        expected = {
            "moedex",
            "codex-app-server",
            "codex-code-mode-host",
            "codex-command-runner",
            "codex-responses-api-proxy",
            "codex-windows-sandbox-setup",
        }
        if actual != expected:
            raise ValueError(
                f"combinedSymbols inventory mismatch: expected {sorted(expected)}, got {sorted(actual)}"
            )
    else:
        symbol_specs = {
            "cliSymbols": (
                f"codex-symbols-{target}",
                {"moedex", "codex-code-mode-host", "codex-responses-api-proxy"},
            ),
            "appServerSymbols": (
                f"codex-symbols-{target}-app-server",
                {"codex-app-server", "codex-code-mode-host"},
            ),
        }
        for role, (root, expected) in symbol_specs.items():
            actual = _symbol_names(archives[role], root, target)
            if actual != expected:
                raise ValueError(
                    f"{role} inventory mismatch: expected {sorted(expected)}, got {sorted(actual)}"
                )


def _validate_package(
    package_root: Path, expected_variant: str
) -> tuple[list[str], dict[str, Any] | None]:
    errors: list[str] = []
    manifest_path, path_errors = _resolve_regular_file(
        package_root, "codex-package.json", "package manifest"
    )
    errors.extend(path_errors)
    if manifest_path is None:
        return errors, None
    try:
        metadata = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        return [*errors, f"invalid package manifest: {error}"], None
    entrypoint = metadata.get("entrypoint")
    if metadata.get("layoutVersion") != 1:
        errors.append("package layoutVersion must be 1")
    if metadata.get("variant") != expected_variant:
        errors.append(f"unexpected package variant: {metadata.get('variant')!r}")
    target = metadata.get("target")
    if not isinstance(target, str) or not target:
        errors.append("package target must be nonempty")
        target = ""
    supported_targets = {
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
        "aarch64-pc-windows-msvc",
        "x86_64-pc-windows-msvc",
    }
    if target and target not in supported_targets:
        errors.append(f"unsupported release package target: {target}")
    suffix = ".exe" if "windows" in target else ""
    entrypoint_stem = "moedex" if expected_variant == "codex" else "codex-app-server"
    expected_entrypoint = f"bin/{entrypoint_stem}{suffix}"
    if entrypoint != expected_entrypoint:
        errors.append(
            f"unexpected package entrypoint: expected {expected_entrypoint!r}, got {entrypoint!r}"
        )
    if not isinstance(metadata.get("version"), str) or not metadata["version"]:
        errors.append("package version must be nonempty")
    if (
        metadata.get("pathDir") != "codex-path"
        or metadata.get("resourcesDir") != "codex-resources"
    ):
        errors.append("package helper directories do not match layout version 1")
    provenance = metadata.get("provenance")
    if not isinstance(provenance, dict):
        errors.append("package provenance must be an object")
    else:
        for key, expected in {
            "product": "moedex",
            "repository": "zak-keown/moedex",
        }.items():
            if provenance.get(key) != expected:
                errors.append(
                    f"invalid package provenance {key}: {provenance.get(key)!r}"
                )
        for key in ("forkCommit", "upstreamCommit", "releaseChannel"):
            if not isinstance(provenance.get(key), str) or provenance[
                key
            ].strip().lower() in {"", "unknown"}:
                errors.append(f"package provenance {key} is missing")
    checksums = metadata.get("checksums")
    if not isinstance(checksums, dict):
        errors.append("package checksums must be an object")
        return errors, metadata
    required_helpers = [
        f"bin/codex-code-mode-host{suffix}",
        f"codex-path/rg{suffix}",
    ]
    if "linux" in target:
        required_helpers.append("codex-resources/bwrap")
    if "windows" in target:
        required_helpers.extend(
            [
                "codex-resources/codex-command-runner.exe",
                "codex-resources/codex-windows-sandbox-setup.exe",
            ]
        )
    if "apple-darwin" in target:
        required_helpers.append("codex-resources/zsh/bin/zsh")
    required_helpers.append(expected_entrypoint)
    for relative in required_helpers:
        helper, helper_errors = _resolve_regular_file(
            package_root, relative, "required package helper"
        )
        errors.extend(helper_errors)
        if (
            helper is not None
            and os.name != "nt"
            and target
            and "windows" not in target
        ):
            if (
                helper.stat().st_mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
                == 0
            ):
                errors.append(f"required package helper is not executable: {relative}")
    payloads = {
        path.relative_to(package_root).as_posix()
        for path in package_root.rglob("*")
        if path.is_file() and path.name != "codex-package.json"
    }
    if set(checksums) != payloads:
        errors.append("package checksum manifest does not exactly cover payloads")
    expected_bins = {expected_entrypoint, f"bin/codex-code-mode-host{suffix}"}
    actual_bins = {path for path in payloads if path.startswith("bin/")}
    if actual_bins != expected_bins:
        errors.append(
            f"package bin inventory mismatch: expected {sorted(expected_bins)}, got {sorted(actual_bins)}"
        )
    expected_path_helpers = {f"codex-path/rg{suffix}"}
    actual_path_helpers = {path for path in payloads if path.startswith("codex-path/")}
    if actual_path_helpers != expected_path_helpers:
        errors.append(
            "package PATH helper inventory mismatch: "
            f"expected {sorted(expected_path_helpers)}, got {sorted(actual_path_helpers)}"
        )
    expected_resource_files: set[str] = set()
    if "linux" in target:
        expected_resource_files.add("codex-resources/bwrap")
    if "windows" in target:
        expected_resource_files.update(
            {
                "codex-resources/codex-command-runner.exe",
                "codex-resources/codex-windows-sandbox-setup.exe",
            }
        )
    actual_resource_files = {
        path
        for path in payloads
        if path.startswith("codex-resources/") and path.count("/") == 1
    }
    if actual_resource_files != expected_resource_files:
        errors.append(
            "package platform helper inventory mismatch: "
            f"expected {sorted(expected_resource_files)}, got {sorted(actual_resource_files)}"
        )
    if "apple-darwin" in target:
        zsh_resource = "codex-resources/zsh/bin/zsh"
        actual_resources = {
            path for path in payloads if path.startswith("codex-resources/")
        }
        if expected_variant == "codex":
            if not all(
                path == zsh_resource or path.startswith("codex-resources/voice/")
                for path in actual_resources
            ):
                errors.append("primary macOS resource inventory contains unknown files")
            errors.extend(_validate_voice_resources(package_root, metadata))
        elif actual_resources != {zsh_resource}:
            errors.append(
                "app-server macOS resource inventory mismatch: "
                f"expected {[zsh_resource]}, got {sorted(actual_resources)}"
            )
    for relative, expected in checksums.items():
        safe, safe_errors = _safe_relative_path(relative, "package checksum path")
        errors.extend(safe_errors)
        if safe is None:
            continue
        path, file_errors = _resolve_regular_file(package_root, safe, "package payload")
        errors.extend(file_errors)
        if path is not None and (
            not isinstance(expected, str) or _sha256(path) != expected
        ):
            errors.append(f"package checksum mismatch: {relative}")
    return errors, metadata


def _validate_voice_resources(
    package_root: Path, package_metadata: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    voice_root = package_root / "codex-resources/voice"
    required = {
        "bin/codex-voice-host",
        "runtime.json",
        "NOTICE.md",
        "sources.json",
        "lib/libgstreamer-1.0.0.dylib",
        "licenses/LGPL-2.1.txt",
        "licenses/Opus.txt",
        "licenses/PCRE2.md",
        "licenses/libffi.txt",
        "licenses/proxy-libintl.txt",
        "licenses/sljit.txt",
        "licenses/zlib.txt",
    }
    for relative in sorted(required):
        _, path_errors = _resolve_regular_file(
            voice_root, relative, "required voice resource"
        )
        errors.extend(path_errors)
    manifest_path, path_errors = _resolve_regular_file(
        voice_root, "manifest.json", "required voice resource"
    )
    errors.extend(path_errors)
    if manifest_path is None:
        return errors
    try:
        runtime_files = validate_voice_runtime_files(
            voice_root.resolve(strict=True),
            package_metadata.get("target"),
            public_release=True,
        )
        runtime_receipt = json.loads(
            (voice_root / "runtime.json").read_text(encoding="utf-8")
        )
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [*errors, f"invalid voice runtime: {error}"]
    provenance = package_metadata.get("provenance", {})
    if runtime_receipt.get("sourceCommit") != provenance.get("forkCommit"):
        errors.append("voice runtime source commit does not match package provenance")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        return [*errors, f"invalid voice manifest: {error}"]
    expected_fields = {
        "schemaVersion": 1,
        "buildCommit": provenance.get("forkCommit"),
        "appTarget": package_metadata.get("target"),
        "voiceTarget": package_metadata.get("target"),
        "appVersion": package_metadata.get("version"),
    }
    for field, expected in expected_fields.items():
        if manifest.get(field) != expected:
            errors.append(
                f"voice manifest {field} mismatch: expected {expected!r}, "
                f"got {manifest.get(field)!r}"
            )
    digests = manifest.get("sha256")
    if not isinstance(digests, dict):
        return [*errors, "voice manifest sha256 must be an object"]
    voice_payloads = {
        path.relative_to(package_root).as_posix()
        for path in voice_root.rglob("*")
        if path.is_file() and path != manifest_path
    }
    expected_payloads = {package_metadata.get("entrypoint"), *voice_payloads}
    if set(digests) != expected_payloads:
        errors.append("voice manifest does not exactly cover app and voice payloads")
    for relative, expected in runtime_files.items():
        manifest_relative = f"codex-resources/voice/{relative}"
        if digests.get(manifest_relative) != expected:
            errors.append(
                f"voice manifest does not bind validated runtime: {manifest_relative}"
            )
    for relative, expected in digests.items():
        safe, safe_errors = _safe_relative_path(relative, "voice checksum path")
        errors.extend(safe_errors)
        if safe is None:
            continue
        path, file_errors = _resolve_regular_file(
            package_root, safe, "voice manifest payload"
        )
        errors.extend(file_errors)
        if path is not None and (
            not isinstance(expected, str) or _sha256(path) != expected
        ):
            errors.append(f"voice manifest checksum mismatch: {relative}")
    return errors


def verify_artifacts(
    manifest: dict[str, Any], artifact_root: Path
) -> tuple[list[str], list[dict[str, Any]]]:
    errors: list[str] = []
    package_metadata: list[dict[str, Any]] = []
    cli_errors, cli = _validate_package(artifact_root / "cli", "codex")
    app_errors, app = _validate_package(
        artifact_root / "app-server", "codex-app-server"
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
                    f"{entry['id']} artifact requires exactly one of: "
                    + ", ".join(artifact["paths"])
                )
                continue
            relative, path = matches[0]
            if artifact["executable"] and os.name != "nt":
                mode = path.stat().st_mode
                if mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH) == 0:
                    errors.append(f"artifact is not executable: {relative}")
    return errors, package_metadata


def verify(
    manifest_path: Path, repo_root: Path, artifact_dir: Path | None = None
) -> tuple[list[str], list[dict[str, Any]]]:
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
    if any(
        entry.get("status") == "broken" for entry in manifest.get("requirements", [])
    ):
        errors.append("manifest contains a broken requirement")
    return errors, package_metadata


def write_evidence(
    manifest_path: Path,
    output: Path,
    target: str,
    channel: str,
    fork_commit: str,
    archives: dict[str, Path],
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
        raise ValueError(
            "release evidence must record every archive qualification gate exactly once"
        )
    _validate_release_archives(target, archives)
    provenance = package_metadata[0]["provenance"]
    evidence = {
        "schemaVersion": 1,
        "product": "Moedex",
        "manifestSha256": _sha256(manifest_path),
        "forkCommit": fork_commit,
        "upstreamBase": manifest["upstreamBase"],
        "releaseChannel": channel,
        "targetTriple": target,
        "archives": {
            role: {"name": path.name, "sha256": _sha256(path)}
            for role, path in sorted(archives.items())
        },
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
    output.write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def run_package_journeys(repo_root: Path) -> int:
    required_environment = {
        "MOEDEX_PACKAGE_TARGET": "--package-target",
        "MOEDEX_CLI_ARCHIVE": "--cli-archive",
        "MOEDEX_APP_SERVER_ARCHIVE": "--app-server-archive",
        "MOEDEX_CLI_SYMBOLS_ARCHIVE": "--cli-symbols-archive",
        "MOEDEX_APP_SERVER_SYMBOLS_ARCHIVE": "--app-server-symbols-archive",
    }
    missing = [name for name in required_environment if not os.environ.get(name)]
    if missing:
        print(
            "error: packaged journey environment is missing: " + ", ".join(missing),
            file=sys.stderr,
        )
        return 2
    command = ["uv", "run", "--frozen", "pytest", "-v", "--compression", "gzip"]
    for environment, option in required_environment.items():
        command.extend([option, os.environ[environment]])
    command.extend(["test_codex_package.py", "test_symbol_archives.py"])
    return subprocess.run(
        command,
        cwd=repo_root / "scripts/codex_package/smoke_tests",
        check=False,
    ).returncode


def rust_gate_listing_argv(gate: dict[str, Any]) -> list[str] | None:
    argv = gate["argv"]
    if argv[:2] != ["just", "test"]:
        return None
    return ["cargo", "nextest", "list", "--message-format", "json", *argv[2:]]


def selected_test_count(listing: str) -> int:
    try:
        payload = json.loads(listing)
    except json.JSONDecodeError as error:
        raise ValueError("nextest listing was not valid JSON") from error
    suites = payload.get("rust-suites")
    if not isinstance(suites, dict):
        raise ValueError("nextest listing omitted rust-suites")
    return sum(
        1
        for suite in suites.values()
        if isinstance(suite, dict)
        for testcase in suite.get("testcases", {}).values()
        if isinstance(testcase, dict)
        and testcase.get("filter-match", {}).get("status") == "matches"
    )


def verify_rust_gate_selection(gate: dict[str, Any], cwd: Path) -> tuple[bool, str]:
    listing_argv = rust_gate_listing_argv(gate)
    if listing_argv is None:
        return True, ""
    result = subprocess.run(
        listing_argv,
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        return (
            False,
            f"test listing failed with {result.returncode}: {result.stderr.strip()}",
        )
    try:
        count = selected_test_count(result.stdout)
    except ValueError as error:
        return False, str(error)
    if count == 0:
        return False, "focused Rust gate selected zero tests"
    return True, ""


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
    evidence_parser.add_argument("--cli-archive", type=Path, required=True)
    evidence_parser.add_argument("--app-server-archive", type=Path, required=True)
    symbols = evidence_parser.add_mutually_exclusive_group(required=True)
    symbols.add_argument("--combined-symbols-archive", type=Path)
    symbols.add_argument("--cli-symbols-archive", type=Path)
    evidence_parser.add_argument("--app-server-symbols-archive", type=Path)
    evidence_parser.add_argument("--passed-gate", action="append", required=True)
    evidence_parser.add_argument("--output", type=Path, required=True)
    gates_parser = subparsers.add_parser("run-gates")
    gates_parser.add_argument("--gate", action="append", required=True)
    subparsers.add_parser("package-journeys")
    args = parser.parse_args(argv)

    errors, package_metadata = verify(
        args.manifest, args.repo_root, getattr(args, "artifact_dir", None)
    )
    if not errors:
        ancestry_commit = args.fork_commit if args.command == "evidence" else "HEAD"
        errors.extend(
            verify_git_ancestry(
                load_manifest(args.manifest), args.repo_root, ancestry_commit
            )
        )
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1
    manifest = load_manifest(args.manifest)
    if args.command == "package-journeys":
        return run_package_journeys(args.repo_root)
    if args.command == "run-gates":
        unknown = sorted(set(args.gate) - set(manifest["gates"]))
        if unknown:
            print(f"error: unknown gates: {', '.join(unknown)}", file=sys.stderr)
            return 1
        for name in args.gate:
            gate = manifest["gates"][name]
            gate_cwd = args.repo_root / gate["cwd"]
            selection_ok, selection_error = verify_rust_gate_selection(gate, gate_cwd)
            if not selection_ok:
                print(
                    f"error: gate {name} {selection_error}",
                    file=sys.stderr,
                )
                return 4
            print(f"running gate {name}: {' '.join(gate['argv'])}", flush=True)
            result = subprocess.run(gate["argv"], cwd=gate_cwd, check=False)
            if result.returncode:
                print(
                    f"error: gate {name} failed with {result.returncode}",
                    file=sys.stderr,
                )
                return result.returncode
    if args.command == "evidence":
        archives = {
            "cliPackage": args.cli_archive,
            "appServerPackage": args.app_server_archive,
        }
        if args.combined_symbols_archive is not None:
            if args.app_server_symbols_archive is not None:
                parser.error(
                    "--app-server-symbols-archive cannot accompany --combined-symbols-archive"
                )
            archives["combinedSymbols"] = args.combined_symbols_archive
        else:
            if args.app_server_symbols_archive is None:
                parser.error(
                    "--app-server-symbols-archive is required with --cli-symbols-archive"
                )
            archives["cliSymbols"] = args.cli_symbols_archive
            archives["appServerSymbols"] = args.app_server_symbols_archive
        try:
            write_evidence(
                args.manifest,
                args.output,
                args.target,
                args.channel,
                args.fork_commit,
                archives,
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
