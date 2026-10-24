# MCP server

Cardbe can run a [Model Context Protocol](https://modelcontextprotocol.io) server inside the desktop app, so AI tools on the same computer can read and update boards. It is off by default, listens only on `127.0.0.1`, and accepts only apps that you add in Settings, each with its own access token and board permissions.

The server is part of the Cardbe process; there is no separate executable or sidecar. It supports the Streamable HTTP transport with `initialize`, `tools/list`, and `tools/call`. Resources, prompts, sampling, elicitation, and an MCP client are not implemented.

## Turning it on

1. Open **Settings → Preferences…** (`Ctrl+,`) and select **MCP**.
2. Turn on **Enable MCP server**. The server starts immediately and the status shows **Running**.
3. Optional: change **Port** (default `8765`, allowed `1024`–`65535`) and select **Apply**. A running server restarts on the new port.
4. Optional: select **Start when Cardbe opens**. Otherwise an enabled server starts only when you turn it on or select **Start**.

If the port is already in use, Cardbe shows an error and keeps the setting; choose another port or free it, then select **Start**. Other Cardbe features keep working when the server cannot start or stops unexpectedly (status **Stopped unexpectedly**).

Development instances started with `CARDBE_DEV_PORT` default to `8765 + (CARDBE_DEV_PORT - 1420)` so that instances from one checkout do not collide. Settings are stored per data directory.

## Adding an app and getting its token

1. Under **Apps**, select **Add app**.
2. Enter a name and choose the boards it may access: **All boards** (the default, which also covers boards added later) or only the boards you select. Optionally allow it to make changes (add columns and change tasks). Apps are read-only unless you allow changes.
3. Select **Add**. Cardbe shows the access token once in the settings panel, with **Copy token** and **Copy client configuration**.

Tokens are saved in the operating system's secure storage (Windows Credential Manager, macOS Keychain, or the Secret Service on Linux) so you can copy them again from the app's menu. When secure storage is unavailable, the token is shown only once; copy it before closing the panel.

The **Recent calls** list in the MCP tab shows the last 200 tool calls since Cardbe started: time, app, tool, board, result and duration. Arguments and results are not recorded, and the list is not saved.

To revoke access, use **Regenerate token** (the old token stops working on the next request) or **Remove app**. Changing an app's boards or permissions applies to its next request without restarting the server.

## Connecting a client

The server address is shown in Settings, by default:

```text
http://127.0.0.1:8765/mcp
```

Every request must carry `Authorization: Bearer <token>`. Configuration formats differ between clients and change over time; check your client's documentation if an example below does not match your version.

**Claude Code** (command line):

```sh
claude mcp add --transport http cardbe http://127.0.0.1:8765/mcp --header "Authorization: Bearer <token>"
```

**Claude Code project file** (`.mcp.json`). This is also what **Copy client configuration** produces:

```json
{
  "mcpServers": {
    "cardbe": {
      "type": "http",
      "url": "http://127.0.0.1:8765/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

**Cursor** (`~/.cursor/mcp.json`): the same `mcpServers` entry; Cursor infers the transport from `url`.

**VS Code** (`.vscode/mcp.json`), prompting for the token instead of storing it in the file:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "cardbe-token",
      "description": "Cardbe MCP token",
      "password": true
    }
  ],
  "servers": {
    "cardbe": {
      "type": "http",
      "url": "http://127.0.0.1:8765/mcp",
      "headers": { "Authorization": "Bearer ${input:cardbe-token}" }
    }
  }
}
```

Clients that only launch local stdio servers (for example, Claude Desktop's configuration file) need a bridge such as [`mcp-remote`](https://www.npmjs.com/package/mcp-remote): `npx mcp-remote http://127.0.0.1:8765/mcp --header "Authorization: Bearer <token>"`. Bridges are third-party software and are not tested with Cardbe.

Do not commit configuration files that contain a token.

## Tools

| Tool            | Access | Description                                                       |
| --------------- | ------ | ----------------------------------------------------------------- |
| `board_list`    | Read   | Boards the app may access, with this device's role on each        |
| `column_list`   | Read   | A board's columns in display order                                |
| `column_create` | Write  | Add a custom-ordered column at the end or before another column   |
| `column_update` | Write  | Rename, recolor, or re-sort a column, or make it the done column  |
| `column_move`   | Write  | Move a column before another column or to the end                 |
| `task_list`     | Read   | Search or page through tasks (`active`, `archived`, `all`, ...)   |
| `task_get`      | Read   | One task with its description and checklist                       |
| `task_create`   | Write  | Create a task at the end of a column                              |
| `task_update`   | Write  | Change fields of an active task; `null` clears optional fields    |
| `task_move`     | Write  | Move a task to a position in a column                             |
| `task_archive`  | Write  | Archive a task; recurring tasks continue with the next occurrence |
| `task_restore`  | Write  | Restore an archived task to a column                              |
| `task_complete` | Write  | Move a task to the done column, or archive it if there is none    |
| `task_reopen`   | Write  | Move a done or archived task back to another column               |

Times are Unix epoch milliseconds. `board_list` also returns `hidden_board_count`, the number of boards the app was not granted, so a client can tell that boards exist without seeing their names. `task_list` pages hold up to `limit` tasks (1 to 100, default 50) and include `next_offset` while more tasks follow; it is absent on the last page. `task_create` and `column_create` accept an optional `idempotency_key` (1 to 128 bytes): retrying with the same key and arguments returns what the first call created (as it is now) instead of a duplicate, and the same key with other arguments is rejected. Keys belong to the app and tool that used them and are kept in memory for a day; they do not survive a restart. There is no delete tool: archive tasks instead, and `task_restore` brings them back. Write tools are not listed for read-only apps. Input and output JSON Schemas come from the Rust types and are returned by `tools/list`.

Columns report `sort_rules`, the display order (up to three distinct fields among `due_time`, `start_time`, `created_time`, `title`, `checklist_progress` and `priority`, each `asc` or `desc`, most significant first, after pinned tasks); `column_update` replaces them and `[]` restores the manual order. `sort_order` summarizes them for older clients. Like a drag in the app, `task_move` also works on sorted columns: the position sets the manual order kept beneath the sort.

Tasks carry an optional `priority` (`low`, `medium`, `high`, `urgent`). Completion follows position: a board has at most one done column (`done` on columns), tasks in it are completed, and their read-only `completed_time` records when they entered it. Moving a task there with `task_move` completes it just like `task_complete`. `blocked_by` lists tasks of the same board that a task waits on; only those still open block it, and writes reject the task itself, unknown tasks, more than 50 tasks, and cycles.

## Security and permissions

- The server binds only to `127.0.0.1`; there is no setting for other interfaces.
- Every request needs a valid Bearer token. Cardbe stores only the token's SHA-256 digest in its database and compares digests in constant time. Tokens are never written to logs.
- `Host` must be `127.0.0.1:<port>` or `localhost:<port>`, and a request with an `Origin` header must come from that same address. This blocks DNS rebinding and web pages in a browser.
- The caller's identity comes from the token, never from request data. Each app sees and changes only its granted boards (with **All boards**, every board on this device, checked on each request); write tools additionally need the write permission, and read-only shared boards reject every write.
- Tools expose only the operations listed above. There is no access to SQL, files, shell commands, settings, sharing, or plugins.
- Changes made through MCP are saved and synchronized like other local edits, but they do not enter Undo. As with plugin changes, the open board's Undo history is cleared after an MCP change so Undo cannot overwrite it.
- Errors return stable codes such as `{"code":"TASK_NOT_FOUND"}`. Internal details stay in the Cardbe log after redaction.

Localhost is not fully trusted: any program running as your user can reach the port. The token is what limits access, so treat it like a password.

## Developer notes

### Architecture

```text
MCP client ─ HTTP ─▶ src-tauri/src/mcp/server.rs   authentication, Host/Origin checks, rmcp handler
                              │
                              ▼
                     src-tauri/src/operations/       registry, dispatcher, grants, DTOs
                     ├── handlers.rs                 one type per operation
                     ├── columns.rs, tasks.rs ◀──── plugin changesets (plugins/board.rs::prepare)
                     └── board_write.rs              storage::persist_local_board_change ◀── plugin commits
                              │
                              ▼
                     board_mutations.rs, commands/archive.rs, storage.rs (SQLite, Loro, sync state)
```

- **Registry and dispatcher** (`operations/mod.rs`): every operation implements the `Operation` trait with a name, description, required `Access`, typed `Input`/`Output` (deserialized with `deny_unknown_fields`, schemas from `schemars`), the board to authorize, and a handler. `OPERATIONS` lists them; `visible()` filters by grants and `dispatch()` deserializes, authorizes, and runs.
- **Plugins** keep their WIT API and changeset model. Their reads use the same `plugins::board::Reader` as the read operations; their create/update/move operations use the same validated functions in `operations/tasks.rs`; their commits use the same `storage::persist_local_board_change`. No WIT or `plugin-api` change was needed.
- **Tauri commands** for the open board keep their own entry points (with Undo and stale-board checks) and share the mutation functions in `board_mutations.rs` and `commands/archive.rs`.
- **Lifecycle** (`mcp/server.rs`): `McpServer` is the single owner of the listener, its cancellation token, and the authorized-client snapshot. `start` is synchronous and idempotent; `stop` cancels and waits up to five seconds. The server runs on Tauri's async runtime, and operations run in blocking workers.
- **Settings** live in `cardbe_global_metadata` under `mcp_server` (`mcp/config.rs`), outside `Settings`, backups, board synchronization, and Undo.

### Adding an operation

1. Add input and output types in `operations/handlers.rs` (inputs with `#[serde(deny_unknown_fields)]`; output types in `operations/dto.rs` must be JSON objects).
2. Implement `Operation`: choose `Access::Read` or `Access::Write`, return the target board from `board()`, and call existing services. Reads use `Reader`; writes use `board_write::commit`, which validates the role, persists, installs the change into the open board, and emits `cardbe:data-changed`.
3. Add it to `OPERATIONS`. It then appears in `tools/list` (dots become underscores, so `task.create` is `task_create`) for every app whose grants allow it; no MCP adapter changes are needed.
4. If plugins need the same capability, put the validated mutation in `operations/tasks.rs` (or reuse one) and call it from both the handler and `plugins/board.rs`. Exposing it to guests requires a reviewed change in the `cardbe-plugin-api` repository first.
5. Extend `operations/tests.rs` and, for protocol behavior, `mcp/server/tests.rs`. Both use isolated temporary databases.

### Tests

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib -- operations:: mcp::
```

The MCP tests start real servers on port `0` and never touch user data or the OS keyring.

### Known limitations

- Only local connections and Bearer tokens are supported; there is no OAuth or remote access.
- Tool descriptions are English, as they are read by AI models rather than shown in Cardbe.
- Running operations are not cancelled when a client disconnects.
- Writes do not ask for confirmation in Cardbe; the per-app write permission is the approval.
