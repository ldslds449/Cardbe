# Cardbe Project Guidelines

## Project Scope and Structure

- Cardbe is a local-first desktop app. Windows is the primary development and validation platform; keep the code portable for potential macOS and Linux support.
- The frontend uses Svelte 5, SvelteKit, TypeScript, and Tailwind CSS 4. The desktop app uses Tauri 2 and Rust, with local SQLite storage. Keep the SvelteKit static adapter and SPA architecture.
- `src/routes/` contains pages, feature components, and state; `src/lib/components/ui/` contains shared UI; `src-tauri/src/` contains Rust, storage, and Tauri commands; `scripts/` contains launch tooling.
- Core features must work offline. Keep sharing, synchronization, and update checks consistent with their existing activation flows; do not make cloud services or accounts prerequisites for core operations.
- Preserve data compatibility, backups, and error handling when changing storage, imports, or synchronization. Tests must not operate on users' production data.

## UI and Implementation

- Use **shadcn-svelte styling**. Reuse existing components in `$lib/components/ui` first, following `components.json` and the theme in `src/app.css`. Do not introduce React-based shadcn/ui components.
- Refer to the official [shadcn-svelte component documentation](https://www.shadcn-svelte.com/docs/components) for component selection, composition, and usage. Adapt examples to installed versions and existing project conventions; documentation is not a reason to overwrite customized components or upgrade dependencies.
- Follow existing Tailwind semantic colors, spacing, radii, light/dark themes, and Lucide icons. Avoid creating another design system or hardcoding arbitrary colors.
- Preserve keyboard interaction, focus management, form labels, and accessibility. Prefer existing Bits UI / shadcn-svelte components for new interactions.
- Trace existing callers and data flow before making the smallest necessary change. Reuse utilities and installed packages; do not add abstractions or dependencies for speculative needs.

## Undo Scope and Planning

- Undo is limited to eligible user operations initiated on the current local computer. Track operation origin explicitly; receiving or persisting a remote change locally does not make it a local operation.
- On shared boards, changes made by other users and received through sharing or synchronization must never enter the local Undo history or be undone. Local operations on shared boards may be eligible, but undoing them must preserve subsequent changes made by other users; do not restore whole-board snapshots that overwrite remote changes.
- Connection settings and language settings are never eligible for Undo and must not enter the Undo history.
- When implementing Undo, validate operation eligibility both when recording history and when applying an undo. Verify local operations, remote shared-board updates, excluded settings, and conflicts with subsequent remote changes using isolated test data.

## Internationalization

- Keep the existing Paraglide / inlang layout: `project.inlang/settings.json` defines supported locales and the catalog path; `messages/{locale}.json` contains translations; `src/lib/i18n/` owns locale resolution, reactive preference, and display formatting. `project.inlang/` and `messages/` follow the tool's standard project layout.
- Use flat, descriptive message keys grouped by feature (for example, `task_due_date` or `calendar_go_today`) and import typed messages from `$lib/paraglide/messages`. Generated `src/lib/paraglide/` files are ignored by Git; never edit them. `vite.config.js` integrates compilation into development, tests, and builds; run `vp run i18n:compile` when generated modules are needed before those flows, such as in a fresh checkout's editor.
- Whenever adding or changing UI text, descriptions, tooltips, accessibility labels, notifications, or displayed values, update **all currently supported languages** (`en` and `zh-TW`) in the same change. Use the existing Paraglide catalogs and typed message functions; do not leave English literals in localized views.
- Localize **numeric values** as well as words: counts, statistics, percentages, decimals, and units must follow the active locale. Use the existing Intl helpers or Paraglide number formatting, and provide correct plural forms for complete messages. Do not assemble sentences from translated fragments.
- Dates, times, month/weekday names, calendars, and human-readable export content must follow the selected language and update when it changes. Preserve machine-readable dates, stored values, IDs, and user-authored content.
- Reuse `getLocale`, `formatDate`, `formatDateTime`, and `formatNumber` from `$lib/i18n`. Keep language preference in the existing reactive locale module and persist it through `set_language`; retain the `system` fallback for old settings. Preserve drafts and focus during language changes without reloading or remounting views. Compilation uses the installed local message-format plugin, and runtime translations must work offline.
- Prefer natural Taiwanese interface wording for `zh-TW`, chosen for the actual state and action (for example, 「尚未建立欄位」 for an empty board and 「跳至今天」 for calendar navigation), instead of translating English word for word.
- Verify both languages, including zero/one/multiple-item statistics, empty states, multiline help, and live language switching. Remove one-off catalog migration scripts after use.

## Dependencies and Lock Files

- **Do not independently edit, delete, regenerate, or format lock files**, including `pnpm-lock.yaml` and `src-tauri/Cargo.lock`. Only update them through the appropriate package manager when the user explicitly requests dependency changes; never edit them manually.
- Use `vp install --frozen-lockfile` for routine installation and `--locked` for Rust validation. If manifests and lock files disagree, report the cause instead of removing these restrictions or updating the locks.
- Use the project's specified pnpm / Vite+ toolchain. Do not mix in npm, yarn, or bun and generate additional lock files.

## Concurrent Development and Test Isolation

- When running multiple desktop instances from one checkout, assign a different port to each. Reuse the isolation in `scripts/tauri.mjs`, `vite.config.js`, and `svelte.config.js`; changing only the Vite port is insufficient without matching Tauri URLs, data directories, and build outputs.
- Start development with `vp run dev` or `vp run tauri dev`. The launcher reuses an idle instance with a persistent random ID, or creates one when all instances are busy. `vp run tauri dev 1421` sets a preferred port; occupied ports automatically fall back to an available port. The selected ID, data directory, and port are printed at startup.
- Keep `strictPort: true`. The launcher selects the port before starting Vite and passes the same port to Tauri. Never terminate another process to free a port.
- Debug data lives in `.cardbe-debug/<random-id>/`. Runtime app identifiers and WebView storage use this stable ID, independently of the port. Local socket reservations isolate instance selection and the launch/build lifetime; the desktop retains its OS data lock and a runtime reservation. Legacy numeric directories are renamed once by the desktop under its data lock; close older desktops before their first migration.
- Build outputs remain isolated by the selected port:

  | Purpose                   | 1420                  | Custom port (e.g. 1421)           |
  | ------------------------- | --------------------- | --------------------------------- |
  | Rust build output         | `src-tauri/target/`   | `src-tauri/target/dev-1421/`      |
  | SvelteKit generated files | `.svelte-kit/`        | `.svelte-kit-dev-1421/`           |
  | Vite cache                | `node_modules/.vite/` | `node_modules/.vite-cardbe-1421/` |

- Do not share SQLite databases between instances or copy production data into test directories. Changing ports must never rename or replace an existing random-ID data directory.
- For standalone frontend development or tests, set `CARDBE_DEV_PORT` in that terminal. To run Rust unit tests alongside development builds, also set a separate `CARGO_TARGET_DIR` (e.g. `src-tauri/target/test-1421`) before running `vp run test`. Unit tests do not need to listen on a port; use the port as the isolation identifier.
- LAN sharing uses an available port by default. If setting `CARDBE_SHARE_DEV_PORT`, use a different sharing port for each instance as well.
- Production frontend builds still share `build/`; release outputs are not currently isolated by port. Do not run concurrent production packaging in one checkout. Use separate checkouts or implement complete output isolation when needed.
- Do not commit generated files, caches, or debug data, or delete another instance's directories. Stop only processes you started.

## Platforms and Validation

- Prioritize testing on Windows. Use Node `path`, Rust `Path` / `PathBuf`, and Tauri path APIs; avoid hardcoded drives, user directories, and path separators.
- Limit platform-specific behavior with existing platform checks or Rust `cfg`, preserving compilable alternatives for other platforms. Do not add Windows-only shell commands to shared launch flows.
- Inspect `package.json` and the actual `vite.config.js`. Vite+ built-ins use `vp <command>`; project scripts use `vp run <script>`. Scripts do not override built-ins.
- For code changes, run `vp check` and `vp test run`, plus `vp run check` and `vp run test` for project-specific Svelte checks, formatting, Rust clippy, and Rust tests. Avoid duplicate test runs where checks overlap, based on the change scope.
- Use `vp run frontend:build` for frontend build validation. When desktop packaging validation is needed, use `vp run tauri build -- --locked`. Report skipped or blocked checks explicitly; do not claim support for untested platforms.
- For documentation-only changes, verify content, paths, commands, and the diff without installing dependencies or generating build artifacts.
