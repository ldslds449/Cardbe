//! Deliberately hostile test component; never installed or shipped.
#![allow(clippy::too_many_arguments)] // Generated canonical ABI flattens WIT records.
mod fixture {
    wit_bindgen::generate!({ path: "../../plugin-api/wit", world: "plugin" });
    struct Fixture;
    export!(Fixture);
    impl Guest for Fixture {
        fn run(context: RunContext) -> Result<RunOutput, PluginError> {
            match context.config_json.as_str() {
                "trap" => panic!("test trap"),
                "loop" => loop {
                    std::hint::spin_loop();
                },
                "memory" => {
                    let bytes = std::hint::black_box(vec![0u8; 129 * 1024 * 1024]);
                    cardbe::plugin::host::log("ALLOWED_LOG", bytes.len() as u32);
                }
                "http" => {
                    cardbe::plugin::host::http_get("https://evil.example/private").map_err(
                        |_| PluginError {
                            code: "HTTP_FAILED".into(),
                            retry_after_seconds: None,
                        },
                    )?;
                }
                "http-get-success" | "http-post-success" => {
                    let response = if context.config_json == "http-post-success" {
                        cardbe::plugin::host::http_post(
                            "https://api.example.com/items",
                            "application/json",
                            b"{\"fixture\":true}",
                        )
                    } else {
                        cardbe::plugin::host::http_get("https://api.example.com/items")
                    }
                    .map_err(|_| PluginError {
                        code: "HTTP_FAILED".into(),
                        retry_after_seconds: None,
                    })?;
                    cardbe::plugin::host::log("ALLOWED_LOG", 1);
                    return Ok(RunOutput {
                        changeset: None,
                        tasks: vec![cardbe::plugin::types::ExternalTask {
                            external_key: "fixture:1".into(),
                            title: format!("HTTP {}", response.status),
                            description: String::from_utf8(response.body).unwrap(),
                            url: "https://api.example.com/items/1".into(),
                            state: "open".into(),
                            labels: vec![],
                            updated_at: "2026-10-09T12:34:56Z".into(),
                            column_id: 1,
                        }],
                        cursor: Some("next".into()),
                    });
                }
                "http-request-success" | "http-request-recover" | "http-request-denied" => {
                    use cardbe::plugin::types::{HttpMethod, HttpRequest};
                    let mut request = HttpRequest {
                        method: HttpMethod::Put,
                        url: "https://api.example.com/items/1".into(),
                        headers: vec![
                            ("Content-Type".into(), "application/json".into()),
                            ("If-Match".into(), "\"v1\"".into()),
                        ],
                        body: Some(b"{\"done\":true}".to_vec()),
                    };
                    if context.config_json == "http-request-denied" {
                        request
                            .headers
                            .push(("Authorization".into(), "Bearer guest".into()));
                    }
                    // Recoverable failures leave the run usable; policy failures do not.
                    let title = match cardbe::plugin::host::http_send(&request) {
                        Ok(response) => format!("HTTP {}", response.status),
                        Err(code) => format!("Recovered {code}"),
                    };
                    return Ok(RunOutput {
                        changeset: None,
                        tasks: vec![cardbe::plugin::types::ExternalTask {
                            external_key: "fixture:request".into(),
                            title,
                            description: String::new(),
                            url: String::new(),
                            state: "open".into(),
                            labels: vec![],
                            updated_at: String::new(),
                            column_id: 1,
                        }],
                        cursor: None,
                    });
                }
                "board-changeset" | "board-post" | "board-post-catch" | "mixed"
                | "schedule-changeset" | "typed-mutations" => {
                    if context.config_json == "schedule-changeset" {
                        assert_eq!(context.trigger, "schedule");
                    }
                    if context.config_json == "board-post"
                        || context.config_json == "board-post-catch"
                    {
                        let response = cardbe::plugin::host::http_post(
                            "https://api.example.com/items",
                            "application/json",
                            b"{}",
                        );
                        if context.config_json == "board-post" {
                            response.map_err(|_| PluginError {
                                code: "HTTP_FAILED".into(),
                                retry_after_seconds: None,
                            })?;
                        }
                    }
                    use cardbe::plugin::{board, types::*};
                    let failure = |_| PluginError {
                        code: "QUERY_FAILED".into(),
                        retry_after_seconds: None,
                    };
                    let response = board::get_board().map_err(failure)?;
                    assert_eq!(response.version, context.board_version);
                    assert_eq!(response.board.id, context.board_id);
                    let boards = board::list_boards().map_err(failure)?;
                    assert_eq!(boards.boards.len(), 1);
                    let columns = board::list_columns().map_err(failure)?;
                    if let Some(column) = columns.columns.first() {
                        assert_eq!(
                            board::get_column(column.id).map_err(failure)?.column.id,
                            column.id
                        );
                    }
                    let mut request = TaskQuery {
                        query: String::new(),
                        column_id: None,
                        status: TaskStatus::All,
                        sort: TaskSort::Title,
                        due: DueFilter::All,
                        due_start: None,
                        due_end: None,
                        archive_start: None,
                        archive_end: None,
                        search_options: None,
                        offset: 0,
                        limit: 100,
                    };
                    let page = board::list_tasks(&request).map_err(failure)?;
                    assert_eq!(page.version, context.board_version);
                    if let Some(task) = page.tasks.first() {
                        assert_eq!(
                            board::get_task(task.task.id, task.archived)
                                .map_err(failure)?
                                .task
                                .task
                                .title,
                            task.task.title
                        );
                    }
                    request.limit = 101;
                    assert!(matches!(
                        board::list_tasks(&request),
                        Err(board::HostError::InvalidArgument)
                    ));
                    request.limit = 1;
                    assert!(board::list_tasks(&request).is_ok());
                    let mut operations = vec![Operation::CreateColumn(CreateColumn {
                        key: "new".into(),
                        name: "Proposed".into(),
                        color: String::new(),
                    })];
                    if context.config_json == "typed-mutations" {
                        let task = page.tasks.first().unwrap().task.clone();
                        operations.extend([
                            Operation::UpdateColumn(UpdateColumn {
                                column: ColumnReference::Temporary("new".into()),
                                name: "Renamed".into(),
                                color: "#123456".into(),
                                sort_order: ColumnSort::Custom,
                            }),
                            Operation::MoveColumn(MoveColumn {
                                column: ColumnReference::Temporary("new".into()),
                                before: None,
                            }),
                            Operation::CreateTask(CreateTask {
                                key: "copy".into(),
                                column: ColumnReference::Temporary("new".into()),
                                task: task.clone(),
                            }),
                            Operation::UpdateTask(UpdateTask {
                                task_id: task.id,
                                task: task.clone(),
                            }),
                            Operation::MoveTask(MoveTask {
                                task_id: task.id,
                                column: ColumnReference::Temporary("new".into()),
                                before_task_id: None,
                            }),
                        ]);
                    }
                    if context.config_json == "mixed" || context.config_json == "typed-mutations" {
                        operations.push(Operation::UpsertExternalTask(UpsertExternalTask {
                            column: Some(ColumnReference::Temporary("new".into())),
                            task: ExternalTask {
                                external_key: "fixture:mixed".into(),
                                title: "Mixed".into(),
                                description: String::new(),
                                url: String::new(),
                                state: "open".into(),
                                labels: vec![],
                                updated_at: String::new(),
                                column_id: 0,
                            },
                        }));
                    }
                    return Ok(RunOutput {
                        tasks: vec![],
                        changeset: Some(Changeset {
                            version: context.board_version,
                            operations,
                        }),
                        cursor: Some("next".into()),
                    });
                }
                "logs-error" => {
                    cardbe::plugin::host::log("ALLOWED_LOG", 3);
                    return Err(PluginError {
                        code: "HTTP_FAILED".into(),
                        retry_after_seconds: None,
                    });
                }
                "messages" => {
                    use cardbe::plugin::host::log_message;
                    use cardbe::plugin::types::LogLevel;
                    log_message(LogLevel::Info, "fetched 3 items\nnext page");
                    log_message(LogLevel::Warn, "token=fixture-secret\u{1b}[31m");
                    log_message(LogLevel::Debug, &format!("{}權", "x".repeat(1022)));
                    for _ in 0..300 {
                        log_message(LogLevel::Error, "spam");
                    }
                    return Err(PluginError {
                        code: "HTTP_FAILED".into(),
                        retry_after_seconds: None,
                    });
                }
                "logs-trap" => {
                    cardbe::plugin::host::log("ALLOWED_LOG", 2);
                    panic!("test trap after log");
                }
                "logs-loop" => {
                    cardbe::plugin::host::log("ALLOWED_LOG", 1);
                    loop {
                        std::hint::spin_loop();
                    }
                }
                "unknown-error" => {
                    return Err(PluginError {
                        code: "SECRET_USER_CONTENT".into(),
                        retry_after_seconds: None,
                    })
                }
                "logs" => {
                    for _ in 0..200 {
                        cardbe::plugin::host::log("ALLOWED_LOG", 1);
                    }
                    cardbe::plugin::host::log("SECRET_USER_CONTENT", 1);
                }
                _ => {}
            }
            Ok(RunOutput {
                changeset: None,
                tasks: vec![],
                cursor: Some("next".into()),
            })
        }
    }
}
