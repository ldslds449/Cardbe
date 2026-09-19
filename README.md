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

Cardbe persists its application data locally in its app-data directory. The repository does not include a sample database or personal task data.

LAN sharing is opt-in: publishing a board starts a local HTTP server reachable by devices on the same network and exposes the selected task content to anyone who has the unguessable share link. Treat the link as sensitive, share only content suitable for that audience, set an expiry where appropriate, and revoke it when finished. Do not use the feature on an untrusted network.

The update checker contacts the GitHub Releases API to check for a newer version. Cardbe does not require an account, cloud sync, or an API key.

## Prerequisites

- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system

## Getting started

```bash
git clone https://github.com/ldslds449/Cardbe.git
cd Cardbe
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts the complete desktop app. `pnpm dev` starts only the Vite frontend, so native features such as tray controls, notifications, file dialogs, and LAN sharing are unavailable there.

## Scripts

| Command | Description |
| --- | --- |
| `pnpm tauri dev` | Run the desktop app in development mode |
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
