# Cardbe

Cardbe is a local-first desktop task manager built around a flexible Kanban board. It runs on your computer with Tauri, SvelteKit, and TypeScript, so your boards, tasks, and notes stay on the device unless you explicitly export or share them.

## Features

- Create a board with custom columns and drag-and-drop task ordering.
- Add labels, checklists, Markdown notes, schedules, recurring tasks, and due dates.
- Search, archive, restore, and focus on work that needs attention.
- Use global shortcuts and the system tray for quick task or note capture.
- Import and export board data as JSON; export calendar data when needed.
- Share selected columns and tasks as a temporary, read-only link on your local network.

## Privacy and LAN sharing

Cardbe persists its application data locally. The repository does not include a sample database or personal task data.

Desktop development builds store their data in `.cardbe-debug/<port>/` at the repository root. The default port is 1420, so `pnpm tauri dev` uses `.cardbe-debug/1420/data.sqlite3`. Release builds continue to use the system app-local-data directory; existing release data is not copied into debug. Release uses Tauri's single-instance plugin. Each Debug port has its own runtime app identifier and data-directory lock, separating WebView storage and preventing two windows from opening the same database at once. The development directory is ignored by Git.

To develop with two data sets from the same checkout, run `pnpm tauri dev` in one terminal and `pnpm tauri dev 1421` in another. The second uses `.cardbe-debug/1421/data.sqlite3`. Both run Tauri's development watcher, so frontend and Rust changes rebuild independently. Additional ports use separate Cargo output under `src-tauri/target/dev-<port>/`; their first build takes longer and uses more disk space. Debug LAN shares use an available port unless `CARDBE_SHARE_DEV_PORT` is set.

LAN sharing is opt-in: publishing a board starts a local HTTP server reachable by devices on the same network and exposes the selected task content to anyone who has the unguessable share link. Treat the link as sensitive, share only content suitable for that audience, set an expiry where appropriate, and revoke it when finished. Do not use the feature on an untrusted network.

The update checker contacts the GitHub Releases API to check for a newer version. Cardbe does not require an account, cloud sync, or an API key.

## Prerequisites

- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system
- [Gitleaks](https://github.com/gitleaks/gitleaks#installing) for the pre-commit secret scan

## Getting started

```bash
git clone https://github.com/ldslds449/Cardbe.git
cd Cardbe
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts the complete desktop app. `pnpm dev` starts only the Vite frontend, so native features such as tray controls, notifications, file dialogs, and LAN sharing are unavailable there.

## Secret scanning

`pnpm install` configures Git to use this repository's hooks. Before every commit, the
`pre-commit` hook runs `gitleaks protect --staged --redact` and rejects staged secrets.
Install the Gitleaks CLI and make sure `gitleaks` is available on your `PATH` before committing.

If hooks were installed before this change, run the following once from the repository root:

```bash
git config core.hooksPath .githooks
```

## Scripts

| Command | Description |
| --- | --- |
| `pnpm tauri dev [port]` | Run an independent desktop development instance; defaults to port 1420 |
| `pnpm test` | Run frontend unit tests |
| `pnpm check` | Run Svelte and TypeScript checks |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Run Rust unit tests |
| `pnpm build` | Build the frontend assets |
| `pnpm tauri build` | Build installable desktop bundles |

Desktop bundles are generated under `src-tauri/target/release/bundle/` and are deliberately excluded from Git.

## Project structure

```text
src/                    SvelteKit frontend
|-- lib/components/ui/  Reusable UI components
`-- routes/             Board state, pages, and feature components

src-tauri/              Tauri and Rust backend
|-- src/commands/       Board, sharing, updates, notifications, and import/export
|-- src/models.rs       Data models and schema migration
`-- src/storage.rs      Local SQLite storage, backups, and recovery
```
