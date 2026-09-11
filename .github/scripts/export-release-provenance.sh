#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_ENV:?GITHUB_ENV must point to the GitHub Actions environment file}"

fork_commit="$(git rev-parse HEAD)"
git fetch --no-tags https://github.com/openai/codex.git main
upstream_commit="$(git merge-base "$fork_commit" FETCH_HEAD)"

for value in "$fork_commit" "$upstream_commit"; do
  if [[ -z "$value" || "$value" == "unknown" ]]; then
    echo "Unable to resolve release provenance" >&2
    exit 1
  fi
done

{
  echo "STABLE_GIT_COMMIT=$fork_commit"
  echo "STABLE_UPSTREAM_GIT_COMMIT=$upstream_commit"
  echo "MOEDEX_RELEASE_CHANNEL=github"
  echo "MOEDEX_REQUIRE_PROVENANCE=1"
} >>"$GITHUB_ENV"
