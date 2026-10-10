//! Deliberately hostile test component; never installed or shipped.
#![allow(clippy::too_many_arguments)] // Generated canonical ABI flattens WIT records.
wit_bindgen::generate!({ path: "../../plugin-api/wit", world: "importer" });
struct Fixture;
export!(Fixture);
impl Guest for Fixture {
    fn run(context: RunContext) -> Result<Changeset, PluginError> {
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
                cardbe::plugin::host::http_get("https://evil.example/private").map_err(|_| {
                    PluginError {
                        code: "HTTP_FAILED".into(),
                        retry_after_seconds: None,
                    }
                })?;
            }
            "http-success" => {
                let response = cardbe::plugin::host::http_get("https://api.example.com/items")
                    .map_err(|_| PluginError {
                        code: "HTTP_FAILED".into(),
                        retry_after_seconds: None,
                    })?;
                cardbe::plugin::host::log("ALLOWED_LOG", 1);
                return Ok(Changeset {
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
        Ok(Changeset {
            tasks: vec![],
            cursor: Some("next".into()),
        })
    }
}
