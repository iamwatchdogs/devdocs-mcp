# devdocs-mcp

A local documentation server for coding agents. It keeps a documentation index in SQLite and serves it over MCP. An agent queries one local source instead of crawling docs sites.

> [!WARNING]
> **Work in progress — not ready for use.** This project is not ready for use. Do not install it. Do not depend on it.
>
> - The CLI, the config format, and the MCP tools can change at any time.
> - A change can break the index you already built.
> - Only people and agents who work on this repo should build and run it.
>
> This banner stays in place until the first release (docs/implementation/12, milestone M9).

## What works today

You can build the CLI from source. You can run the config commands, and you can load a documentation source into the local index with `devdocs add --fixture`. You cannot query the index yet. Retrieval, symbols, and the MCP server land in later milestones (docs/implementation/12).

## Build

You need Rust through rustup (<https://rustup.rs>) and `git`. Repo work also uses `just` and `lefthook`; AGENTS.md carries the full setup.

```sh
cargo build --release --locked
cargo install --path crates/devdocs --locked
lefthook install
```

## Try the commands

```sh
devdocs --version
devdocs config init
```

The license gate is deny-by-default. Register your source in the config file that `config init` wrote:

```toml
[sources.example]
kind = "llms_txt"
url = "https://example.test/llms.txt"
status = "approved_index"
```

```sh
devdocs add https://example.test/llms.txt --fixture <fixture-dir>
devdocs list
```

## Documentation

- `AGENTS.md` — the entry point for agents: commands, layout, and the instruction layer.
- `docs/implementation/00-implementation-guide.md` — the design docs index and its reading order.

The docs describe the full target design. The repo does not match them everywhere yet; the "Scaffold deviations" table lists each intentional gap.

## Scaffold deviations

Intentional deviations between the implementation docs and the repository. Resolved conflicts live in the owning docs, not here.

| Doc says | Repo does | Why |
|---|---|---|
| Structured logs via `tracing` and `tracing-subscriber` ([01](docs/implementation/01-tech-stack-decision.md)) | `Reporter` writes plain lines to stderr under `--verbose` | The subscriber crate is not in the lock yet; add it when the ingest layer emits structured events |
| Help groups subcommands under headings ([08](docs/implementation/08-retrieval-interfaces.md)) | Only `Global options` gets a heading | clap 4.6.7 `next_help_heading` applies to arguments, not subcommands |
| `README.md` stays a stub until M9 ([02](docs/implementation/02-project-scaffolding.md)) | Minimal end-user readme with a work-in-progress banner | End users must see the not-ready state now; the full readme lands at M9 (current-state) |

## License

MIT. See `LICENSE`.
