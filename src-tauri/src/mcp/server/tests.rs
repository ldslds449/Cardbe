use super::*;
use crate::{
    mcp::config::{token_hash, McpClient},
    operations::tests::TestHost,
};
use serde_json::{json, Value};

const PROTOCOL: &str = "2025-06-18";
const WRITER: &str = "cardbe_mcp_writer-token";
const READER: &str = "cardbe_mcp_reader-token";

struct Fixture {
    host: Arc<TestHost>,
    server: McpServer,
    http: reqwest::Client,
}

impl Fixture {
    fn new() -> Self {
        let host = Arc::new(TestHost::new());
        let server = McpServer::new(host.clone(), None);
        let board = host.board_id;
        server.set_clients(&[
            McpClient {
                id: "writer".into(),
                name: "Writer".into(),
                token_hash: token_hash(WRITER),
                all_boards: false,
                boards: [board].into(),
                write: true,
                created_at: 0,
            },
            McpClient {
                id: "reader".into(),
                name: "Reader".into(),
                token_hash: token_hash(READER),
                all_boards: false,
                boards: [board].into(),
                write: false,
                created_at: 0,
            },
        ]);
        Self {
            host,
            server,
            http: reqwest::Client::new(),
        }
    }

    fn start(&self) -> u16 {
        self.server.start(0).unwrap()
    }

    async fn post(
        &self,
        port: u16,
        token: Option<&str>,
        origin: Option<&str>,
        body: Value,
    ) -> reqwest::Response {
        let mut request = self
            .http
            .post(endpoint(port))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL)
            .body(body.to_string());
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        request.send().await.unwrap()
    }

    async fn rpc(&self, port: u16, token: &str, method: &str, params: Value) -> Value {
        let response = self
            .post(
                port,
                Some(token),
                None,
                json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
            )
            .await;
        assert_eq!(response.status(), 200, "{method}");
        response.json().await.unwrap()
    }

    async fn call(&self, port: u16, token: &str, name: &str, arguments: Value) -> Value {
        self.rpc(
            port,
            token,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )
        .await
    }
}

fn initialize() -> Value {
    json!({"protocolVersion": PROTOCOL, "capabilities": {},
           "clientInfo": {"name": "test", "version": "1.0"}})
}

#[tokio::test]
async fn clients_initialize_list_tools_and_create_and_read_tasks() {
    let fixture = Fixture::new();
    let port = fixture.start();
    let initialized = fixture.rpc(port, WRITER, "initialize", initialize()).await;
    assert_eq!(initialized["result"]["serverInfo"]["name"], "cardbe");
    assert!(initialized["result"]["capabilities"]["tools"].is_object());

    let listed = fixture.rpc(port, WRITER, "tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), operations::OPERATIONS.len());
    let create = tools
        .iter()
        .find(|tool| tool["name"] == "task_create")
        .unwrap();
    assert_eq!(create["inputSchema"]["type"], "object");
    assert!(create["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("title")));
    assert_eq!(create["outputSchema"]["type"], "object");
    assert_eq!(create["annotations"]["readOnlyHint"], false);

    let board = fixture.host.board_id;
    let created = fixture
        .call(
            port,
            WRITER,
            "task_create",
            json!({"board_id": board, "column_id": 2, "title": "From MCP 🤖"}),
        )
        .await;
    let result = &created["result"];
    assert_eq!(result["isError"], false);
    let id = result["structuredContent"]["task"]["id"].as_i64().unwrap();
    let fetched = fixture
        .call(
            port,
            READER,
            "task_get",
            json!({"board_id": board, "task_id": id}),
        )
        .await;
    assert_eq!(
        fetched["result"]["structuredContent"]["task"]["title"],
        "From MCP 🤖"
    );
    assert_eq!(*fixture.host.changed.lock().unwrap(), vec![board]);
    fixture.server.stop().await;
}

#[tokio::test]
async fn read_only_clients_are_not_offered_or_allowed_writes() {
    let fixture = Fixture::new();
    let port = fixture.start();
    let listed = fixture.rpc(port, READER, "tools/list", json!({})).await;
    let names: Vec<_> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"task_list".to_owned()));
    assert!(!names
        .iter()
        .any(|name| name == "task_create" || name == "task_archive"));
    let denied = fixture
        .call(
            port,
            READER,
            "task_create",
            json!({"board_id": fixture.host.board_id, "column_id": 1, "title": "No"}),
        )
        .await;
    assert_eq!(denied["error"]["code"], -32602);
    assert!(fixture.host.changed.lock().unwrap().is_empty());
    fixture.server.stop().await;
}

#[tokio::test]
async fn unknown_tools_are_protocol_errors_and_bad_arguments_are_tool_errors() {
    let fixture = Fixture::new();
    let port = fixture.start();
    let unknown = fixture.call(port, WRITER, "shell_exec", json!({})).await;
    assert_eq!(unknown["error"]["code"], -32602);

    let invalid = fixture
        .call(
            port,
            WRITER,
            "task_get",
            json!({"board_id": "one", "task_id": 1}),
        )
        .await;
    assert_eq!(invalid["result"]["isError"], true);
    assert_eq!(
        invalid["result"]["structuredContent"],
        json!({"code": "INVALID_ARGUMENT"})
    );

    let other_board = fixture
        .call(
            port,
            WRITER,
            "column_list",
            json!({"board_id": fixture.host.other_board_id}),
        )
        .await;
    assert_eq!(
        other_board["result"]["structuredContent"],
        json!({"code": "PERMISSION_DENIED"})
    );

    let missing = fixture
        .call(
            port,
            WRITER,
            "task_get",
            json!({"board_id": fixture.host.board_id, "task_id": 999}),
        )
        .await;
    assert_eq!(
        missing["result"]["structuredContent"],
        json!({"code": "TASK_NOT_FOUND"})
    );

    let malformed = fixture
        .http
        .post(endpoint(port))
        .bearer_auth(WRITER)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", PROTOCOL)
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert!(malformed.status().is_client_error());

    // Recorded newest first, without arguments; unknown tools are not tools.
    let activity = fixture.server.activity();
    let summary: Vec<_> = activity
        .iter()
        .map(|call| {
            (
                call.client_id.as_str(),
                call.tool.as_str(),
                call.board_id,
                call.error.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                "writer",
                "task_get",
                Some(fixture.host.board_id),
                Some("TASK_NOT_FOUND")
            ),
            (
                "writer",
                "column_list",
                Some(fixture.host.other_board_id),
                Some("PERMISSION_DENIED")
            ),
            ("writer", "task_get", None, Some("INVALID_ARGUMENT")),
        ]
    );
    let recorded = serde_json::to_value(&activity[0]).unwrap();
    let mut fields: Vec<_> = recorded.as_object().unwrap().keys().cloned().collect();
    fields.sort();
    assert_eq!(
        fields,
        [
            "at",
            "board_id",
            "client_id",
            "duration_ms",
            "error",
            "tool"
        ]
    );
    fixture.server.clear_activity();
    assert!(fixture.server.activity().is_empty());
    fixture.server.stop().await;
}

#[tokio::test]
async fn requests_without_a_current_token_or_from_web_origins_are_rejected() {
    let fixture = Fixture::new();
    let port = fixture.start();
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});
    for token in [None, Some("cardbe_mcp_wrong"), Some("")] {
        let response = fixture.post(port, token, None, body.clone()).await;
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    for origin in ["https://example.com", "http://localhost:1420", "null"] {
        let response = fixture
            .post(port, Some(WRITER), Some(origin), body.clone())
            .await;
        assert_eq!(response.status(), 403, "{origin}");
    }
    let rebinding = fixture
        .http
        .post(endpoint(port))
        .bearer_auth(WRITER)
        .header("Host", "attacker.example")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(rebinding.status(), 403);
    let same_origin = fixture
        .post(
            port,
            Some(WRITER),
            Some(&format!("http://127.0.0.1:{port}")),
            body.clone(),
        )
        .await;
    assert_eq!(same_origin.status(), 200);

    // Revocation applies to the next request without restarting.
    fixture.server.set_clients(&[]);
    let revoked = fixture.post(port, Some(WRITER), None, body).await;
    assert_eq!(revoked.status(), 401);
    fixture.server.stop().await;
}

#[tokio::test]
async fn server_reports_port_conflicts_and_starts_stops_and_restarts() {
    let fixture = Fixture::new();
    let occupied = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let busy = occupied.local_addr().unwrap().port();
    assert!(matches!(
        fixture.server.start(busy),
        Err(DomainError::McpPortUnavailable)
    ));
    assert_eq!(fixture.server.status(), (Status::Stopped, None));

    let port = fixture.start();
    assert_eq!(fixture.server.status(), (Status::Running, Some(port)));
    // A second start keeps the running server.
    assert_eq!(fixture.server.start(busy).unwrap(), port);
    fixture.server.stop().await;
    assert_eq!(fixture.server.status(), (Status::Stopped, None));
    assert!(fixture.http.post(endpoint(port)).send().await.is_err());

    let restarted = fixture.server.start(port).unwrap();
    assert_eq!(restarted, port);
    let listed = fixture.rpc(port, WRITER, "tools/list", json!({})).await;
    assert!(listed["result"]["tools"].is_array());
    fixture.server.stop().await;
    // Stopping twice is harmless.
    fixture.server.stop().await;
}
