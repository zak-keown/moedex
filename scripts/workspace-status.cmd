@echo off
setlocal

if defined STABLE_GIT_COMMIT (
  set "BUILD_COMMIT=%STABLE_GIT_COMMIT%"
) else (
  for /f "delims=" %%I in ('git rev-parse --verify HEAD 2^>nul') do set "BUILD_COMMIT=%%I"
  if not defined BUILD_COMMIT if defined GITHUB_SHA set "BUILD_COMMIT=%GITHUB_SHA%"
  if not defined BUILD_COMMIT set "BUILD_COMMIT=unknown"
)

echo STABLE_GIT_COMMIT %BUILD_COMMIT%
if defined STABLE_UPSTREAM_GIT_COMMIT (
  echo STABLE_UPSTREAM_GIT_COMMIT %STABLE_UPSTREAM_GIT_COMMIT%
) else (
  echo STABLE_UPSTREAM_GIT_COMMIT unknown
)
if defined MOEDEX_RELEASE_CHANNEL (
  echo MOEDEX_RELEASE_CHANNEL %MOEDEX_RELEASE_CHANNEL%
) else (
  echo MOEDEX_RELEASE_CHANNEL github
)
