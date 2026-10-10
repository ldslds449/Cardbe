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

## Application Themes

- Keep theme metadata in `src/lib/theme/themes.ts` and localized names in both message catalogs. Reuse the shadcn-svelte Button for theme selection with pressed state, keyboard activation, focus styling, and CSS-based palette previews.
- `theme-manager.svelte.ts` owns the selected preference; `theme-provider.svelte` uses mode-watcher as the sole runtime owner of the root `.dark` class, color scheme, and `data-theme`. System resolves to Classic Light/Dark; fixed themes ignore OS appearance changes. Preserve the public share viewer's Classic Light/Dark toggle.
- Persist the optional `Settings.theme` through existing SQLite settings metadata and `set_theme`, outside board synchronization and Undo. Missing values migrate the existing mode-watcher preference; invalid IDs resolve to System.
- Apply selection immediately, but update the `cardbe-theme` startup cache only after successful backend writes. On failure, restore the previous selection and show a translated command error. Keep the backend authoritative, propagate desktop changes through the Tauri event, and reload settings on window focus.
- Validate and restore the startup cache in `src/app.html` before the visible startup screen and coordinate mode-watcher storage. Keep palette tokens in `static/themes.css`, loaded in the document head, and Tailwind semantic mappings in `src/app.css`. Preserve AA text contrast, existing light/dark status colors, and independent user-authored colors and export palettes.
- When adding a theme, update the registry, both catalogs, CSS tokens, startup validation list, and backend allowlist in `src-tauri/src/commands/settings.rs`. Preserve validation at both trust boundaries and extend allowlist/parity tests; run theme, contrast, startup, and persistence tests with the relevant project checks.

## Hybrid Task Search

- Keep `search_tasks` (current board) and `list_all_tasks` (task explorer) as entry points. Reuse `src-tauri/src/search.rs`, `storage.rs`, and existing command boundaries; SQLite remains the only index source, with no separate fuzzy cache or migration.
- Preserve the shared eligibility CTE for both engines and final retrieval, including board, column, status, recurrence, due/archive dates, and archived visibility. Board search includes active and archived tasks; explorer search applies its requested status filter. Filter before each engine's Top K.
- Run SQLite trigram FTS5 and Nucleo title matching independently. FTS searches title, description, labels, and checklist text using quoted phrases and BM25; queries shorter than three Unicode characters retain the substring fallback. Nucleo streams only identity/title, scans all eligible titles, and retains bounded Top K using literal, case-insensitive fuzzy matching with smart Unicode normalization. Do not pass descriptions to Nucleo or assume typo correction, phonetic matching, or Chinese segmentation.
- Fuse one-based rankings with RRF `sum(1 / (rrf_k + rank))`, never by adding raw engine scores. Exact case-insensitive titles precede title prefixes, then other matches; use RRF within each group and unique SQLite search-row identity for ties. Preserve `(board_id, task_id, archived)` identity and deduplicate board output by task ID.
- Preserve relevance-ordered board IDs and existing board layout/membership behavior. The explorer selects a fused pool, then applies the user's title/due/column/archive ordering and OFFSET pagination; decode full task payloads only for the final page. Empty explorer queries retain unrestricted filtered pagination.
- Keep independently defaulted options: `candidate_limit = 1000`, `result_limit = 1000`, and `rrf_k = 60`, each an integer in `1..=10_000`. Board search accepts `options`; explorer uses `filter.search_options`. Candidate limits apply per engine; result limits cap the fused pool independently of page size. Keep Rust and TypeScript contracts aligned and return structured `INVALID_ARGUMENT` for invalid ranges.
- Open read-only connections inside blocking workers and use one SQLite read transaction for the complete search. Lock application state only to obtain the database path and validate board context; recheck the active board before returning board results. Preserve frontend debounce and request/board-generation guards against stale results; running workers are not currently cancelled.
- Preserve fresh results after local edits, deletion, archive/restore, board movement, and received remote snapshots through existing SQLite updates/triggers, without new cache hooks. Keep Unicode handling safe for Chinese, mixed scripts, and emoji; do not treat byte or UTF-16 offsets as character indices.
- When changing search, extend existing ranking/storage/frontend tests for exact/prefix/fuzzy/description and shared-engine hits, ties, deduplication, cross-board ID collisions, limits and invalid options, empty/no-match queries, Unicode, filtering before Top K, pagination, local mutations, and received Iroh snapshots. Keep structured command errors and existing locale translation.
- Title scanning is O(N) per query with O(K) retained candidates, and each explorer page repeats the scan. Default caps can omit matches beyond 1,000. Measure release performance before adding caches or claiming latency improvements. Run the ignored `hybrid_search_benchmark` alone against isolated synthetic databases with `CARDBE_DEV_PORT` and a separate `CARGO_TARGET_DIR`: `cargo test --manifest-path src-tauri/Cargo.toml --locked hybrid_search_benchmark --lib -- --ignored --nocapture` (add `--release` for optimized measurements). Stage timings exclude reader opening, IPC, worker queueing, and rendering; Windows peak working set includes fixture setup, the harness, and SQLite caches.

## Plugin Runtime

- The public plugin WIT API is maintained in the `src-tauri/plugin-api` submodule. Do not edit a copied WIT contract in Cardbe; API changes belong in the API repository, followed by a reviewed submodule pointer update. Keep API, package, and host storage schema versions independent.
- Cardbe runs WebAssembly components through Wasmtime. Plugins are optional; core local operations must remain usable offline. Do not expose SQLite connections, app data paths, arbitrary Tauri commands, WASI filesystem/network/process/environment/clock access, or raw host exceptions to guests.
- Route guest network access through the existing host HTTP capability: HTTPS only, approved domains, public-address validation and pinning, no redirects or proxies, bounded requests/responses, and cancellation/time limits. Keep credentials in the host and never expose them to guests.
- Keep manifest and configuration validation at both frontend and backend trust boundaries. Secrets must not be defaults or returned to the guest. Keep plugin error codes/resources isolated from core translations; map host failures to safe host codes and guest failures to structured `PLUGIN_ERROR` values.
- Plugin data and execution history are local to this device. Preserve existing task ownership, user overrides, and normal board persistence/sync behavior. Plugin runs are not eligible for local Undo; do not restore whole-board snapshots or overwrite remote edits.
- The Wasm guest test fixture lives under `src-tauri/test-fixtures/runtime-fixture`. Install `wasm-tools` 1.259.0 (`cargo install wasm-tools --version 1.259.0 --locked`) and build the ignored component with `pnpm plugins:test` before running Rust plugin runtime tests. Use synthetic databases and fixture HTTP only; never use production data or real credentials. `pnpm check` covers formatting and clippy for the fixture crate, and `pnpm test` builds the fixture before Rust tests.
- Production plugins are developed and released in their own repositories. Cardbe tests host compatibility against the fixture and pinned external artifacts; do not fetch a floating WIT/API version or build/publish a production plugin from this repository.

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

## Error Contracts

- Every fallible application Tauri command returns the serde-tagged `CommandError` in `src-tauri/src/errors.rs`, with stable `SCREAMING_SNAKE_CASE` codes and typed public fields. This includes archive, templates, notes, settings, notifications, task explorer, import/export, calendar fonts, LAN/Iroh sharing, diagnostics, and external links. Keep service/storage failures in `DomainError` or internal error types; convert only at the Tauri boundary. Do not use frontend translation keys, localized sentences, HTTP statuses, or raw exception strings as command contracts.
- Map expected failures explicitly (missing entities, stale board context, permissions, required board name, last board). Unexpected failures become `INTERNAL_ERROR`; log backend details through existing diagnostic redaction. Do not expose paths, database details, credentials, raw config, stack traces, or arbitrary user content in IPC errors. Expected failures do not need error-level logs.
- Use `parseCommandError(unknown)` and `translateCommandError` from `$lib/command-errors` for application command failures. The translator resolves current-locale Paraglide messages and leaves toast/inline/dialog presentation to callers. Unknown, malformed, and legacy thrown values use the localized internal-error fallback; never display `String(error)` to users. Keep both catalogs updated when adding codes.
- Preserve the independent `PluginHostError` boundary: host permission/runtime failures map to safe host codes; plugin-owned errors use `PLUGIN_ERROR` with `plugin_id` and nested `plugin_error`. Plugin resources are isolated by plugin ID, locale, and plugin error code; they cannot override core messages. Missing or broken plugin translations use the generic plugin failure.
- `PluginUserError::new` validates plugin codes and limits params to 16 shallow JSON scalar fields and 4096 serialized UTF-8 bytes; frontend parsing applies the same limits. A future host must additionally allowlist public params against each plugin schema and exclude secrets/exceptions. Do not persist raw runtime errors in future background execution records.
- Update checking, startup failure reporting, and Iroh join/access/sync also use structured command errors. Never infer i18n or UI actions from backend wording (`startsWith`, regex, message substrings). Legacy Iroh peer compatibility belongs only in the Rust wire adapter; approval carries `device_id`, and revocation has its own IPC code. Keep the v1 peer wire format until capability negotiation supports additional codes. Store structured errors for persistent inline UI and translate during rendering so language changes update the message.
- Do not add or retain `Result<T, String>` at application command boundaries. Internal diagnostic strings may remain private, but expected failures must preserve typed variants through services/storage instead of being flattened into wording. Recovery notifications return structured `RecoveryNotice` values; raw recovery details stay in backend logs. Test command signature coverage, Rust/frontend code parity, serialization, parameter preservation, privacy, malformed/unknown values, plugin namespace isolation, and live locale changes. Do not introduce a plugin SDK or new error dependencies just for this contract.
- The LAN public-share JSON API also returns serialized `CommandError` on failure with an appropriate HTTP status. The viewer parses the code and translates during rendering; never wrap backend wording in `Error`. Keep operation-specific retry prompts, but include the translated command failure when available.
- Iroh join/access/sync repository calls preserve boxed domain errors with `DomainError::repository`; do not flatten them with `to_string()`. `ShareError::Internal` maps to `INTERNAL_ERROR`.

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
- Paraglide output in `src/lib/paraglide/` is also shared across ports. Run i18n compilation, Svelte checks, frontend tests, and builds sequentially in one checkout; overlapping compilation can replace message declarations while another check reads them.
- Do not commit generated files, caches, or debug data, or delete another instance's directories. Stop only processes you started.

## Platforms and Validation

- Prioritize testing on Windows. Use Node `path`, Rust `Path` / `PathBuf`, and Tauri path APIs; avoid hardcoded drives, user directories, and path separators.
- Limit platform-specific behavior with existing platform checks or Rust `cfg`, preserving compilable alternatives for other platforms. Do not add Windows-only shell commands to shared launch flows.
- Inspect `package.json` and the actual `vite.config.js`. Vite+ built-ins use `vp <command>`; project scripts use `vp run <script>`. Scripts do not override built-ins.
- For code changes, run `vp check` and `vp test run`, plus `vp run check` and `vp run test` for project-specific Svelte checks, formatting, Rust clippy, and Rust tests. Avoid duplicate test runs where checks overlap, based on the change scope.
- Use `vp run frontend:build` for frontend build validation. When desktop packaging validation is needed, use `vp run tauri build -- --locked`. Report skipped or blocked checks explicitly; do not claim support for untested platforms.
- For documentation-only changes, verify content, paths, commands, and the diff without installing dependencies or generating build artifacts.
