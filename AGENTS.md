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

## Dependencies and Lock Files

- **Do not independently edit, delete, regenerate, or format lock files**, including `pnpm-lock.yaml` and `src-tauri/Cargo.lock`. Only update them through the appropriate package manager when the user explicitly requests dependency changes; never edit them manually.
- Use `vp install --frozen-lockfile` for routine installation and `--locked` for Rust validation. If manifests and lock files disagree, report the cause instead of removing these restrictions or updating the locks.
- Use the project's specified pnpm / Vite+ toolchain. Do not mix in npm, yarn, or bun and generate additional lock files.

## Concurrent Development and Test Isolation

- When running multiple desktop instances from one checkout, assign a different port to each. Reuse the isolation in `scripts/tauri.mjs`, `vite.config.js`, and `svelte.config.js`; changing only the Vite port is insufficient without matching Tauri URLs, data directories, and build outputs.
- Start the default instance with `vp run dev` (1420). In another terminal, start an additional instance with `vp run tauri dev 1421`; further instances can use 1422, etc. `scripts/dev.mjs` currently does not forward port arguments, so use the tauri script for additional instances.
- Keep `strictPort: true`. If a port is occupied, select another explicit port; do not silently switch ports or terminate someone else's process.
- Preserve these existing isolation paths. Port 1420 retains the default directories; custom ports use separate directories:

  | Purpose                   | 1420                  | Custom port (e.g. 1421)           |
  | ------------------------- | --------------------- | --------------------------------- |
  | Debug data                | `.cardbe-debug/1420/` | `.cardbe-debug/1421/`             |
  | Rust build output         | `src-tauri/target/`   | `src-tauri/target/dev-1421/`      |
  | SvelteKit generated files | `.svelte-kit/`        | `.svelte-kit-dev-1421/`           |
  | Vite cache                | `node_modules/.vite/` | `node_modules/.vite-cardbe-1421/` |

- Keep debug runtime app identifiers, WebView storage, and data-directory locks isolated by port. Do not share SQLite databases between instances or copy production data into test directories.
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
