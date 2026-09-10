#!/usr/bin/env bash

set -euo pipefail

if [[ -n "${STABLE_GIT_COMMIT:-}" ]]; then
    build_commit="${STABLE_GIT_COMMIT}"
elif build_commit="$(git rev-parse --verify HEAD 2>/dev/null)"; then
    :
elif [[ -n "${GITHUB_SHA:-}" ]]; then
    build_commit="${GITHUB_SHA}"
else
    build_commit="unknown"
fi

printf 'STABLE_GIT_COMMIT %s\n' "${build_commit}"
printf 'STABLE_UPSTREAM_GIT_COMMIT %s\n' "${STABLE_UPSTREAM_GIT_COMMIT:-unknown}"
printf 'MOEDEX_RELEASE_CHANNEL %s\n' "${MOEDEX_RELEASE_CHANNEL:-github}"
