export interface McpClient {
  id: string;
  name: string;
  /** Includes boards added later; `boards` is then empty. */
  all_boards: boolean;
  boards: number[];
  write: boolean;
  created_at: number;
}

/** One recorded tool call; arguments and results are not kept. */
export interface McpActivity {
  at: number;
  client_id: string;
  tool: string;
  board_id: number | null;
  /** Public command error code; null when the call succeeded. */
  error: string | null;
  duration_ms: number;
}

export interface McpServer {
  enabled: boolean;
  auto_start: boolean;
  port: number;
  endpoint: string;
  status: "stopped" | "running" | "failed";
  clients: McpClient[];
}

export interface McpClientToken {
  server: McpServer;
  client_id: string;
  token: string;
  token_saved: boolean;
}

/** Matches the backend: unprivileged ports only. */
export function valid_port(port: number): boolean {
  return Number.isInteger(port) && port >= 1024 && port <= 65535;
}

/** `mcpServers` entry for clients that accept HTTP servers with headers. */
export function client_configuration(endpoint: string, token: string): string {
  return JSON.stringify(
    {
      mcpServers: {
        cardbe: {
          type: "http",
          url: endpoint,
          headers: { Authorization: `Bearer ${token}` },
        },
      },
    },
    null,
    2,
  );
}
