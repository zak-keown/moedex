# Moedex

Moedex is an independent fork of OpenAI Codex (https://github.com/zak-keown/moedex). It is a coding harness for sustained engineering and research that runs locally on your computer.

## Run locally

Build the public executable from this checkout:

```shell
cd codex-rs
cargo build -p codex-cli --bin moedex
./target/debug/moedex --help
```

The intended npm package is `@zak-keown/moedex`; it exposes `moedex` and does not install a `codex` alias. It is not published from this repository yet.

Moedex uses its own local state by default, allowing it to coexist with a stock Codex installation. Authentication remains with the selected provider: use `moedex login` to sign in with ChatGPT or configure an API key.

## Attribution and compatibility

Moedex preserves upstream attribution and its Apache-2.0 license. Internal `codex-*` crate names, OpenAI/ChatGPT provider labels, app-server protocol names, and existing rollout history remain where they describe compatibility, providers, or historical data.

| Remaining term | Why it remains |
| --- | --- |
| `codex-*`, `CODEX_*`, and internal paths | Compatibility-facing implementation identifiers. |
| OpenAI, ChatGPT, model names, and Codex Cloud | Provider, model, or upstream service labels. |
| App-server protocol names and rollout history | Stable protocol and historical data. |
| OpenAI Codex | Upstream attribution. |

Upstream source: [OpenAI Codex](https://github.com/openai/codex). This repository is licensed under the [Apache-2.0 License](LICENSE).
