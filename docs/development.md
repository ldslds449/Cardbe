# Development guide

[Back to README](../README.md)

Cardbe is a local-first desktop app built with Tauri 2, Rust, SvelteKit, Svelte 5, TypeScript, and SQLite. Contributor guidelines live in [AGENTS.md](../AGENTS.md).

## Prerequisites

- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system
- [Gitleaks](https://github.com/gitleaks/gitleaks#installing) for the pre-commit secret scan

## Getting started

```bash
git clone --recurse-submodules https://github.com/ldslds449/Cardbe.git
cd Cardbe
pnpm install
pnpm dev
```

`pnpm dev` starts the complete desktop app with non-incremental Rust builds so a compiler-cache wrapper such as kache can reuse artifacts, and accepts the same preferred port as `vp run tauri dev`. Configure the wrapper itself in your Cargo configuration. `pnpm frontend:dev` starts only the Vite frontend, so native features such as tray controls, notifications, file dialogs, and LAN sharing are unavailable there.

If Rust compilation puts pressure on memory, limit parallel compilation in that terminal. In PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = "2"
vp run tauri dev
```

This trades compilation speed for lower peak memory usage. Adjust the value for your machine; remove the override with `Remove-Item Env:CARGO_BUILD_JOBS`. Quick task and note windows are created and loaded at startup, then hidden and reused for fast first access.

## Development data and multiple instances

Desktop development builds store their data in `.cardbe-debug/<random-id>/` at the repository root. The launcher reuses an idle instance or creates one when all instances are busy, and prints its ID, data directory, and port. The stable ID also isolates WebView storage and the runtime app identifier, independently of the port. Existing release data is not copied into debug. The development directory is ignored by Git.

To develop with two data sets from the same checkout, run `vp run dev` in one terminal and `vp run tauri dev 1421` in another. The launcher selects separate instances and falls back to an available port if the preferred port is busy. Additional ports isolate Cargo output, SvelteKit generated files, and Vite caches; their first build takes longer and uses more disk space. Debug LAN shares use an available port unless `CARDBE_SHARE_DEV_PORT` is set.

## Secret scanning

`pnpm install` configures Git to use this repository's hooks. Before every commit, the
`pre-commit` hook runs the formatting check, frontend lint, and
`gitleaks protect --staged --redact`; it rejects formatting errors, lint errors, and staged secrets.
Install the Gitleaks CLI and make sure `gitleaks` is available on your `PATH` before committing.

If hooks were installed before this change, run the following once from the repository root:

```bash
git config core.hooksPath .githooks
```

## Scripts

| Command                                           | Description                                                              |
| ------------------------------------------------- | ------------------------------------------------------------------------ |
| `pnpm dev [port]`                                 | Run the desktop app with compiler-cache-friendly settings                |
| `pnpm frontend:dev`                               | Run only the Vite frontend                                               |
| `pnpm format`                                     | Format frontend files with Prettier and Rust files with rustfmt          |
| `pnpm lint`                                       | Check frontend and Rust lint rules                                       |
| `pnpm lint:fix`                                   | Apply frontend and Rust lint fixes                                       |
| `vp run tauri dev [port]`                         | Run an isolated desktop instance, optionally requesting a preferred port |
| `pnpm frontend:test`                              | Run frontend unit tests                                                  |
| `pnpm check`                                      | Run frontend, formatting, and Rust checks                                |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Run Rust unit tests                                                      |
| `pnpm frontend:build`                             | Build the frontend assets                                                |
| `pnpm tauri build`                                | Build installable desktop bundles                                        |

Desktop bundles are generated under `src-tauri/target/release/bundle/` and are deliberately excluded from Git.

## Plugins

The public plugin WIT API is maintained in the [cardbe-plugin-api](https://github.com/ldslds449/cardbe-plugin-api) repository and included as the `src-tauri/plugin-api` submodule. Building the plugin runtime test fixture requires `wasm-tools`; see [AGENTS.md](../AGENTS.md#plugin-runtime) for details.

## Project structure

```text
src/                    SvelteKit frontend
|-- lib/components/ui/  Reusable UI components
`-- routes/             Board state, pages, and feature components

src-tauri/              Tauri and Rust backend
|-- plugin-api/         Plugin WIT API (submodule)
|-- src/commands/       Board, sharing, updates, notifications, and import/export
|-- src/plugins/        Plugin packages and WebAssembly runtime
|-- src/models.rs       Data models and schema migration
`-- src/storage.rs      Local SQLite storage, backups, and recovery
```
