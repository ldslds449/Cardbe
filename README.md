# Cardbe

Cardbe is a local-first desktop task manager built around a flexible Kanban board. It runs on your computer with Tauri, SvelteKit, and TypeScript, so your boards, tasks, and notes stay on the device unless you explicitly export or share them.

## Features

- Create a board with custom columns and drag-and-drop task ordering.
- Add labels, checklists, Markdown notes, schedules, recurring tasks, and due dates.
- Search, archive, restore, and focus on work that needs attention.
- Use global shortcuts and the system tray for quick task or note capture.
- Import and export board data as JSON; export calendar data when needed.
- Share selected columns and tasks as a temporary, read-only link on your local network.
- Share an entire board with a read-only or editor invitation link and a locally generated QR code.

## Privacy and LAN sharing

Cardbe persists its application data locally. The repository does not include a sample database or personal task data.

Desktop development builds store their data in `.cardbe-debug/<random-id>/` at the repository root. The launcher reuses an idle instance or creates one when all instances are busy, and prints its ID, data directory, and port. The stable ID also isolates WebView storage and the runtime app identifier, independently of the port. Release builds use the system app-local-data directory; existing release data is not copied into debug. The development directory is ignored by Git.

To develop with two data sets from the same checkout, run `vp run dev` in one terminal and `vp run tauri dev 1421` in another. The launcher selects separate instances and falls back to an available port if the preferred port is busy. Additional ports isolate Cargo output, SvelteKit generated files, and Vite caches; their first build takes longer and uses more disk space. Debug LAN shares use an available port unless `CARDBE_SHARE_DEV_PORT` is set.

LAN sharing is opt-in: publishing a board starts a local HTTP server reachable by devices on the same network and exposes the selected task content to anyone who has the unguessable share link. Treat the link as sensitive, share only content suitable for that audience, set an expiry where appropriate, and revoke it when finished. Do not use the feature on an untrusted network.

The update checker contacts the GitHub Releases API to check for a newer version. Cardbe does not require an account, cloud sync, or an API key.

When a shared-board invitation is active, Cardbe connects to Iroh relay services for peer discovery and transport. Disabling or deleting every invitation stops the owner sharing endpoint.

## Shared boards

Board invitations are bearer capabilities: anyone who receives an active link can use its current permission. A received board is stored separately and marked `read only` or `shared` in the sidebar. Shared boards sync automatically while Cardbe is active and online; **Sync now** remains available for an immediate refresh. Read-only boards receive the owner's current snapshot when its revision or permission changes; unchanged polls receive an empty result. Editors exchange Loro document updates over Iroh with the board owner, so offline changes to different cards or fields merge when they reconnect to the owner. Editors cannot exchange updates directly while the owner is offline. SQLite remains the local queryable copy; Loro is the mergeable source for shared board content. Its Tree stores column order, card order, and card moves; Map stores individual fields; Text stores descriptions for a future Loro rich-text editor in Svelte. Concurrent changes to the same scalar field use Loro's deterministic map conflict rule.

Invitation secrets, device identity, and received-board connection metadata stay in the local SQLite database and are intentionally excluded from JSON backups. This avoids exporting access to another person's board by accident. Each installation has a stable device identity; invitations use that identity with relay-based discovery rather than persisting peer IP addresses, so reconnecting after an IP or NAT change can use a fresh route. The sharing endpoint starts only while an invitation is active. Global app settings are excluded from shared-board snapshots. Restoring a JSON backup on another device intentionally creates a new identity and requires a new invitation.

Revoking an invitation stops future sync, but cannot erase board data already downloaded by a recipient. Loro documents can retain deleted text in their edit history, including invitation snapshots; do not treat deleting a card as secure erasure from shared copies.

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
pnpm dev
```

`pnpm dev` starts the complete desktop app with sccache enabled. `pnpm frontend:dev` starts only the Vite frontend, so native features such as tray controls, notifications, file dialogs, and LAN sharing are unavailable there.

If Rust compilation puts pressure on memory, limit parallel compilation in that terminal. In PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = "2"
vp run tauri dev
```

This trades compilation speed for lower peak memory usage. Adjust the value for your machine; remove the override with `Remove-Item Env:CARGO_BUILD_JOBS`. Quick task and note windows are created and loaded at startup, then hidden and reused for fast first access.

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
| `pnpm dev`                                        | Run the desktop app with sccache                                         |
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
