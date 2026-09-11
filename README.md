# Moedex

Moedex is an independent fork of [OpenAI Codex](https://github.com/openai/codex). It is a local coding harness for sustained engineering and research, distributed from [zak-keown/moedex](https://github.com/zak-keown/moedex).

## Install

Release packages cover these six targets:

| Operating system | Architectures |
| --- | --- |
| macOS | Apple silicon (`aarch64-apple-darwin`), Intel (`x86_64-apple-darwin`) |
| Linux | ARM64 (`aarch64-unknown-linux-musl`), x86-64 (`x86_64-unknown-linux-musl`) |
| Windows | ARM64 (`aarch64-pc-windows-msvc`), x86-64 (`x86_64-pc-windows-msvc`) |

Install a pinned GitHub release on macOS or Linux, replacing `0.1.0` with the version you intend to run:

```shell
curl -fsSL https://github.com/zak-keown/moedex/releases/download/rust-v0.1.0/install.sh \
  | MOEDEX_RELEASE=0.1.0 MOEDEX_NON_INTERACTIVE=1 sh
```

The equivalent PowerShell command is:

```powershell
$env:MOEDEX_RELEASE = "0.1.0"
$env:MOEDEX_NON_INTERACTIVE = "1"
irm https://github.com/zak-keown/moedex/releases/download/rust-v0.1.0/install.ps1 | iex
```

Each installer selects the matching target archive, verifies its release and archive digests, and exposes the `moedex` command. Release CI validates the embedded per-payload checksums. Release assets retain per-target behavior evidence tying the package and symbol archive hashes to the fork commit, upstream base, and GitHub release channel.

To build from source instead:

```shell
git clone https://github.com/zak-keown/moedex.git
cd moedex/codex-rs
cargo build --release -p codex-cli --bin moedex
./target/release/moedex --version
```

The proposed npm name is `@zak-keown/moedex`, but npm installation and automatic npm updates are disabled until ownership and publication are verified. Moedex does not fall back to `@openai/codex`, the stock Homebrew cask, WinGet, or OpenAI's release storage. Current updates come only from GitHub Releases in `zak-keown/moedex`.

## Local state and Codex import

Moedex resolves its effective home in this order:

1. A nonempty `MOEDEX_HOME`.
2. A nonempty `CODEX_HOME` compatibility override.
3. `~/.moedex`.

Blank variables are treated as unset. An invalid explicit path fails instead of silently falling back. Setting `CODEX_HOME` can deliberately share state with stock Codex; check the effective home shown by Moedex diagnostics before writing or deleting data. Repository-local `.codex` configuration and skills retain their compatibility names and precedence.

Stock Codex data is never migrated automatically. Preview an explicit, copy-only import first:

```shell
moedex import codex --dry-run --settings --sessions
```

Then select only the categories you want:

```shell
moedex import codex --settings --sessions
moedex import codex --credentials
```

Credential import is separately selected and defaults off. Conflicts default to skip. Choosing replacement creates a destination backup before replacement. Import leaves the source home unchanged, reports incompatible items individually, removes secrets and home-bound settings from copied configuration, and preserves rollout history needed to resume imported sessions.

## Updates and uninstall

The installer records whether the installed release follows `latest` or is pinned. Update checks and reinstall guidance use `zak-keown/moedex` GitHub Releases. An unavailable or unconfigured update channel leaves the current installation intact.

To remove installer-owned commands and package links, use the installer uninstall entrypoint:

```shell
sh install.sh --uninstall
```

```powershell
./install.ps1 -Uninstall
```

Uninstall preserves Moedex settings, sessions, credentials, logs, and caches. It also preserves every stock Codex command and all stock Codex data. If you later decide to delete Moedex data, first inspect diagnostics and confirm the effective home is a Moedex-owned path. Delete that directory only as a separate, deliberate action; never use a shared `CODEX_HOME` path for cleanup.

## Provider and upstream attribution

Moedex preserves the Apache-2.0 license, notices, and upstream attribution. Authentication and hosted inference remain services of the provider you select, including OpenAI and ChatGPT where configured. Model names, provider environment variables, app-server protocol fields, rollout records, and internal `codex-*` crate and helper names remain where they describe a provider, historical data, or compatibility contract.

| Remaining term | Why it remains |
| --- | --- |
| `codex-*`, `CODEX_*`, and internal paths | Compatibility-facing implementation identifiers. |
| OpenAI, ChatGPT, model names, and Codex Cloud | Provider, model, or upstream service labels. |
| App-server protocol names and rollout history | Stable protocol and historical data. |
| OpenAI Codex | Upstream attribution. |

See the [Apache-2.0 License](LICENSE) and the versioned [`moedex-behavior-manifest.json`](moedex-behavior-manifest.json) qualification contract.
