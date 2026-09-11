#!/usr/bin/env python3

from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
UNIX_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release.yml"
WINDOWS_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release-windows.yml"
PROVENANCE_SCRIPT = REPO_ROOT / ".github/scripts/export-release-provenance.sh"


class ReleaseWorkflowTest(unittest.TestCase):
    def test_default_build_uses_hosted_runners_and_assembles_packages(self) -> None:
        unix = UNIX_WORKFLOW.read_text()
        windows = WINDOWS_WORKFLOW.read_text()

        self.assertNotIn("-runners\n", windows)
        self.assertNotIn("-linux-x64-xl", unix)
        self.assertNotIn("github.event.repository.name }}-linux", unix)
        self.assertIn("package-unsigned-macos:", unix)
        self.assertIn("package-unsigned-windows:", windows)

    def test_release_assets_do_not_require_a_fork_zsh_release(self) -> None:
        unix = UNIX_WORKFLOW.read_text()

        self.assertNotIn("CODEX_ZSH_RELEASE_TAG", unix)
        self.assertNotIn("Download packaged zsh manifest", unix)

    def test_dmg_uses_moedex_name_through_every_stage(self) -> None:
        unix = UNIX_WORKFLOW.read_text()

        self.assertNotIn("codex-${{ matrix.target }}.dmg", unix)
        self.assertNotIn("codex-${TARGET}.dmg", unix)

    def test_release_only_updater_tests_run_in_ci(self) -> None:
        unix = UNIX_WORKFLOW.read_text()

        self.assertIn("release-updater-tests:", unix)
        self.assertIn("cargo test -p codex-tui --release", unix)

    def test_every_package_job_requires_real_provenance(self) -> None:
        unix = UNIX_WORKFLOW.read_text()
        windows = WINDOWS_WORKFLOW.read_text()

        self.assertGreaterEqual(unix.count("export-release-provenance.sh"), 3)
        self.assertGreaterEqual(windows.count("export-release-provenance.sh"), 3)

    def test_provenance_export_resolves_fork_and_upstream_commits(self) -> None:
        script = PROVENANCE_SCRIPT.read_text()

        self.assertIn('fork_commit="$(git rev-parse HEAD)"', script)
        self.assertIn("https://github.com/openai/codex.git main", script)
        self.assertIn('upstream_commit="$(git merge-base', script)
        self.assertIn("STABLE_GIT_COMMIT=$fork_commit", script)
        self.assertIn("STABLE_UPSTREAM_GIT_COMMIT=$upstream_commit", script)
        self.assertIn("MOEDEX_RELEASE_CHANNEL=github", script)
        self.assertIn("MOEDEX_REQUIRE_PROVENANCE=1", script)

    def test_default_release_qualifies_built_archives_with_smoke_suite(self) -> None:
        unix = UNIX_WORKFLOW.read_text()
        qualification = unix.split("\n  qualify-release-packages:\n", 1)[1].split(
            "\n  stage-npm-packages:\n", 1
        )[0]

        self.assertIn("qualify-release-packages:", unix)
        self.assertIn("scripts/codex_package/smoke_tests", qualification)
        self.assertIn("sdk/python/tests", qualification)
        self.assertIn("--cli-archive", qualification)
        self.assertIn("--app-server-archive", qualification)
        self.assertIn("--cli-symbols-archive", qualification)
        self.assertIn("--app-server-symbols-archive", qualification)
        self.assertIn("${{ matrix.app_server_artifact }}-symbols", qualification)
        self.assertNotIn("not debug_symbols", qualification)
        self.assertIn("test_symbol_archives.py", qualification)
        self.assertIn("test_missing_required_helper_fails_qualification", qualification)
        self.assertNotIn("provisioned-macos-candidate", qualification)
        self.assertIn("needs.qualify-release-packages.result == 'success'", unix)

    def test_release_qualification_retains_behavior_evidence_per_target(self) -> None:
        unix = UNIX_WORKFLOW.read_text()
        qualification = unix.split("\n  qualify-release-packages:\n", 1)[1].split(
            "\n  stage-npm-packages:\n", 1
        )[0]

        self.assertIn("moedex_behavior_manifest.py evidence", qualification)
        self.assertIn("--artifact-dir behavior-artifacts", qualification)
        self.assertIn('fork_commit="$(git rev-parse HEAD)"', qualification)
        self.assertIn('--fork-commit "${fork_commit}"', qualification)
        self.assertIn("--channel github", qualification)
        self.assertEqual(qualification.count("--archive"), 4)
        self.assertIn("moedex-behavior-evidence-${{ matrix.target }}", qualification)

    def test_default_build_has_no_oidc_permission_or_cosign_step(self) -> None:
        unix = UNIX_WORKFLOW.read_text()
        build = unix.split("\n  build:\n", 1)[1].split("\n  build-macos-voice:\n", 1)[0]

        self.assertNotIn("id-token: write", build)
        self.assertNotIn("linux-code-sign", build)

    def test_windows_package_qualification_exposes_dumpbin_before_smoke_tests(
        self,
    ) -> None:
        unix = UNIX_WORKFLOW.read_text()
        qualification = unix.split("\n  qualify-release-packages:\n", 1)[1].split(
            "\n  stage-npm-packages:\n", 1
        )[0]
        setup_step = """      - name: Expose MSVC tools for symbol qualification
        if: ${{ runner.os == 'Windows' }}
        uses: ./.github/actions/setup-msvc-env
        with:
          target: ${{ matrix.target }}
"""

        self.assertIn(setup_step, qualification)
        self.assertLess(
            qualification.index(setup_step),
            qualification.index("Exercise assembled archives with conflicting PATH"),
        )


if __name__ == "__main__":
    unittest.main()
