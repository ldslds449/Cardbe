use super::*;
fn package(version: &str) -> PluginPackage {
    parse_package(
        &serde_json::to_vec(&serde_json::json!({
            "id":"example", "name":"Example", "version":version,
            "api_version":"1.1.0", "storage_schema_version":1,
        }))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn the_package_entry_installs_new_ids_and_only_previews_newer_installed_versions() {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&c).unwrap();
    let old = package("1.0.0");
    assert!(update_candidate(&c, &old).unwrap().is_none());
    storage::put(&c, "package", &old.id, &old).unwrap();
    let new = package("1.1.0");
    assert_eq!(
        update_candidate(&c, &new).unwrap().unwrap().version,
        "1.0.0"
    );
    assert!(matches!(
        update_candidate(&c, &old),
        Err(CommandError::PluginUpdateNotNewer)
    ));
    assert!(matches!(
        update_candidate(&c, &package("0.9.0")),
        Err(CommandError::PluginUpdateNotNewer)
    ));
    let mut different = new.clone();
    different.id = "another".into();
    assert!(update_candidate(&c, &different).unwrap().is_none());
    assert_eq!(
        storage::list::<PluginPackage>(&c, "package").unwrap()[0].version,
        "1.0.0"
    );
    assert!(storage::revision(&c, &old.id).unwrap().is_none());
    let installed = serde_json::to_value(PluginPackageSelection::Installed {
        package: old.clone(),
    })
    .unwrap();
    assert_eq!(installed["kind"], "installed");
    let update = serde_json::to_value(PluginPackageSelection::Update {
        current_package: old,
        preview: Box::new(PluginUpdatePreview {
            token: "preview".into(),
            current_version: "1.0.0".into(),
            requires_review: false,
            package: new,
        }),
    })
    .unwrap();
    assert_eq!(update["kind"], "update");
    assert_eq!(update["current_package"]["version"], "1.0.0");
    assert_eq!(update["preview"]["package"]["version"], "1.1.0");
}
#[test]
fn plugin_updates_validate_identity_schema_and_numeric_release_order() {
    let old = package("1.9.9");
    assert!(validate_update(&old, &package("1.10.0")).is_ok());
    for version in ["1.9.9", "1.9.8", "0.99.0"] {
        assert!(matches!(
            validate_update(&old, &package(version)),
            Err(CommandError::PluginUpdateNotNewer)
        ));
    }
    for version in [
        "1.10",
        "01.10.0",
        "1.10.0-beta",
        "1.10.0+build",
        "1. 10.0",
        "18446744073709551616.0.0",
    ] {
        assert!(release_version(version).is_none());
    }
    for (field, api_version) in [
        ("id", ""),
        ("api_version", "2.0.0"),
        ("api_version", "1.0.0"),
        ("storage_schema_version", ""),
    ] {
        let mut next = package("2.0.0");
        match field {
            "id" => next.id = "other".into(),
            "api_version" => next.api_version = api_version.into(),
            _ => next.storage_schema_version = 2,
        }
        let error = validate_update(&old, &next).unwrap_err();
        match field {
            "id" => assert!(matches!(error, CommandError::InvalidArgument)),
            "api_version" => assert!(matches!(
                error,
                CommandError::PluginPackageInvalid {
                    reason: PluginPackageIssue::UnsupportedApiVersion,
                    ..
                }
            )),
            _ => assert!(matches!(
                error,
                CommandError::PluginPackageInvalid {
                    reason: PluginPackageIssue::UnsupportedStorageSchema,
                    ..
                }
            )),
        }
    }
    assert!(validate_update(&old, &package("2.0.0")).is_ok());
}
#[test]
fn rejected_manifests_report_safe_specific_reasons() {
    let base = serde_json::json!({
        "id":"example", "name":"Example", "version":"1.0.0",
        "api_version":"1.1.0", "storage_schema_version":1,
    });
    let reject = |change: &dyn Fn(&mut serde_json::Value)| {
        let mut manifest = base.clone();
        change(&mut manifest);
        serde_json::to_value(parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap_err())
            .unwrap()
    };
    let issue = |reason: &str| serde_json::json!({"code":"PLUGIN_PACKAGE_INVALID","reason":reason});
    assert_eq!(
        serde_json::to_value(parse_package(b"{not json").unwrap_err()).unwrap(),
        issue("MANIFEST_MALFORMED")
    );
    assert_eq!(
        serde_json::to_value(parse_package(&vec![b' '; 65537]).unwrap_err()).unwrap(),
        issue("MANIFEST_TOO_LARGE")
    );
    assert_eq!(reject(&|m| m["id"] = "Bad ID".into()), issue("INVALID_ID"));
    assert_eq!(
        reject(&|m| m["version"] = "v1".into()),
        issue("INVALID_VERSION")
    );
    assert_eq!(
        reject(&|m| m["api_version"] = "1.0.0".into()),
        serde_json::json!({
            "code":"PLUGIN_PACKAGE_INVALID",
            "reason":"UNSUPPORTED_API_VERSION",
            "api_version":"1.0.0",
        })
    );
    // Arbitrary package text is never echoed back.
    assert_eq!(
        reject(&|m| m["api_version"] = "token=secret".into()),
        issue("UNSUPPORTED_API_VERSION")
    );
    assert_eq!(
        reject(&|m| m["storage_schema_version"] = 2.into()),
        issue("UNSUPPORTED_STORAGE_SCHEMA")
    );
    assert_eq!(
        reject(&|m| m["manifest_version"] = 3.into()),
        issue("UNSUPPORTED_MANIFEST_VERSION")
    );
    assert_eq!(
        reject(&|m| m["domains"] = serde_json::json!(["http://bad"])),
        issue("INVALID_DOMAINS")
    );
    assert_eq!(
        reject(&|m| m["error_codes"] = serde_json::json!(["lower"])),
        issue("INVALID_CODES")
    );
    assert_eq!(
        reject(&|m| {
            m["domains"] = serde_json::json!(["api.example.com"]);
            m["settings"] = serde_json::json!([
                {"key":"a","label":"A","type":"secret"},
                {"key":"b","label":"B","type":"secret"},
            ]);
        }),
        issue("LEGACY_CREDENTIALS")
    );
    assert_eq!(
        reject(&|m| m["settings"] = serde_json::json!([
            {"key":"limit","label":"Limit","type":"number","default":"many"},
        ])),
        serde_json::json!({
            "code":"PLUGIN_PACKAGE_INVALID",
            "reason":"INVALID_SETTINGS",
            "setting":"limit",
        })
    );
    assert_eq!(
        reject(&|m| m["settings"] = serde_json::json!([
            {"key":"Bad Key","label":"Bad","type":"text"},
        ])),
        issue("INVALID_SETTINGS")
    );
    assert_eq!(
        reject(&|m| m["http_credentials"] = serde_json::json!([
            {"destination":{"type":"domain","domain":"other.example.com"},"path_prefix":"/"},
        ])),
        issue("INVALID_CREDENTIAL_RULES")
    );
}
#[test]
fn install_requires_updatable_versions_and_typed_setting_defaults() {
    let base = serde_json::json!({
        "id":"example", "name":"Example", "version":"1.0.0",
        "api_version":"1.1.0", "storage_schema_version":1,
    });
    for version in ["v1.0.0", "1.0", "1.0.0-beta", "01.0.0", ""] {
        let mut manifest = base.clone();
        manifest["version"] = version.into();
        assert!(
            parse_package(&serde_json::to_vec(&manifest).unwrap()).is_err(),
            "{version}"
        );
    }
    let field = |kind: &str, default: serde_json::Value| {
        let mut manifest = base.clone();
        manifest["settings"] = serde_json::json!([{
            "key":"field", "label":"Field", "type":kind, "min":1, "max":10,
            "options":[{"value":"open","label":"Open"}], "default":default,
        }]);
        parse_package(&serde_json::to_vec(&manifest).unwrap())
    };
    for (kind, default) in [
        ("text", serde_json::json!("value")),
        ("text", serde_json::Value::Null),
        ("number", serde_json::json!(5)),
        ("boolean", serde_json::json!(true)),
        ("select", serde_json::json!("open")),
        ("board", serde_json::json!(1)),
        ("column", serde_json::json!(2)),
    ] {
        assert!(field(kind, default.clone()).is_ok(), "{kind} {default}");
    }
    for (kind, default) in [
        ("text", serde_json::json!(5)),
        ("text", serde_json::json!("x".repeat(4097))),
        ("number", serde_json::json!("5")),
        ("number", serde_json::json!(11)),
        ("boolean", serde_json::json!("true")),
        ("select", serde_json::json!("closed")),
        ("board", serde_json::json!("1")),
        ("column", serde_json::json!(1.5)),
    ] {
        assert!(field(kind, default.clone()).is_err(), "{kind} {default}");
    }
}
#[test]
fn manifest_versions_default_validate_persist_and_upgrade_independently() {
    let mut manifest = serde_json::to_value(package("1.0.0")).unwrap();
    manifest.as_object_mut().unwrap().remove("manifest_version");
    let old = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(old.manifest_version, 1);
    for version in [
        serde_json::json!(0),
        serde_json::json!(3),
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("2"),
        serde_json::Value::Null,
        serde_json::json!(4294967296_u64),
    ] {
        manifest["manifest_version"] = version;
        assert!(parse_package(&serde_json::to_vec(&manifest).unwrap()).is_err());
    }
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&connection).unwrap();
    let mut legacy = serde_json::to_value(&old).unwrap();
    legacy.as_object_mut().unwrap().remove("manifest_version");
    storage::put(&connection, "package", &old.id, &legacy).unwrap();
    assert_eq!(
        storage::list::<PluginPackage>(&connection, "package").unwrap()[0].manifest_version,
        1
    );
    for version in [1, 2] {
        manifest["manifest_version"] = version.into();
        let current = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(current.manifest_version, version);
        storage::put(&connection, "package", &current.id, &current).unwrap();
        assert_eq!(
            storage::list::<PluginPackage>(&connection, "package").unwrap()[0].manifest_version,
            version
        );
    }
    manifest["version"] = "1.1.0".into();
    let upgraded = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(validate_update(&old, &upgraded).is_ok());
    assert!(update_requires_review(&old, &upgraded));
    assert!(!credentials_changed(&old, &upgraded));
    let mut future = upgraded;
    future.manifest_version = 3;
    assert!(validate_update(&old, &future).is_err());
}
#[test]
fn plugin_update_switches_revision_atomically_and_preserves_local_data() {
    let root = std::env::temp_dir().join(format!("cardbe-update-test-{}", id()));
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    std::fs::write(root.join("component.wasm"), b"old").unwrap();
    let c = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&c).unwrap();
    let old = package("1.0.0");
    storage::put(&c, "package", &old.id, &old).unwrap();
    let mut original = instance();
    original.enabled = true;
    original.config.insert("user_value".into(), "keep".into());
    original.secret_fields = vec!["token".into()];
    original.allowed_domains = vec!["api.example.com".into()];
    original.next_run_at = Some(123);
    storage::put(&c, "instance", &original.id, &original).unwrap();
    let mut other = original.clone();
    other.id = "other".into();
    other.plugin_id = "another".into();
    storage::put(&c, "instance", &other.id, &other).unwrap();
    storage::put(&c, "cursor", &original.id, &Some("cursor")).unwrap();
    storage::put(&c, "run", "run", &serde_json::json!({"instance_id":"a"})).unwrap();
    c.execute(
        "INSERT INTO cardbe_plugin_sources VALUES('a','source',1,2,'{}')",
        [],
    )
    .unwrap();
    let mut update = PreparedPluginUpdate {
        token: id(),
        previous_version: "1.0.0".into(),
        package: package("1.1.0"),
        manifest: b"manifest".to_vec(),
        component: b"new".to_vec(),
    };
    let mut active = std::collections::HashMap::new();
    active.insert(
        original.id.clone(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    assert!(matches!(
        update_locked(&c, &root, &update, &active),
        Err(CommandError::PluginUpdateBusy)
    ));
    assert!(storage::revision(&c, &old.id).unwrap().is_none());
    active.clear();
    c.execute_batch("CREATE TRIGGER fail_update BEFORE INSERT ON cardbe_plugin_data WHEN NEW.kind='revision' BEGIN SELECT RAISE(ABORT,'isolated test failure'); END;").unwrap();
    assert!(update_locked(&c, &root, &update, &active).is_err());
    assert_eq!(
        storage::list::<PluginPackage>(&c, "package").unwrap()[0].version,
        "1.0.0"
    );
    assert_eq!(std::fs::read(root.join("component.wasm")).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    c.execute_batch("DROP TRIGGER fail_update;").unwrap();
    update_locked(&c, &root, &update, &active).unwrap();
    let first = storage::revision(&c, &old.id).unwrap().unwrap();
    assert_eq!(
        std::fs::read(root.join(&first).join("component.wasm")).unwrap(),
        b"new"
    );
    let unchanged = storage::list::<PluginInstance>(&c, "instance").unwrap();
    assert_eq!(
        serde_json::to_value(&unchanged[0]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert!(matches!(
        update_locked(&c, &root, &update, &active),
        Err(CommandError::PluginUpdateStale)
    ));
    update.previous_version = "1.1.0".into();
    update.package.version = "1.2.0".into();
    update.package.domains.push("new.example.com".into());
    c.execute_batch("CREATE TRIGGER fail_review BEFORE INSERT ON cardbe_plugin_data WHEN NEW.kind='revision' BEGIN SELECT RAISE(ABORT,'isolated test failure'); END;").unwrap();
    assert!(update_locked(&c, &root, &update, &active).is_err());
    assert_eq!(storage::revision(&c, &old.id).unwrap(), Some(first.clone()));
    assert_eq!(
        serde_json::to_value(&storage::list::<PluginInstance>(&c, "instance").unwrap()[0]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    c.execute_batch("DROP TRIGGER fail_review;").unwrap();
    update_locked(&c, &root, &update, &active).unwrap();
    assert!(!root.join(first).exists());
    let saved = storage::list::<PluginInstance>(&c, "instance").unwrap();
    assert!(!saved[0].enabled && saved[0].needs_review && saved[0].next_run_at.is_none());
    assert_eq!(saved[0].config, original.config);
    assert_eq!(saved[0].secret_fields, original.secret_fields);
    assert_eq!(saved[0].allowed_domains, original.allowed_domains);
    assert_eq!(saved[0].interval_seconds, original.interval_seconds);
    assert_eq!(
        serde_json::to_value(&saved[1]).unwrap(),
        serde_json::to_value(&other).unwrap()
    );
    assert_eq!(
        storage::list::<String>(&c, "cursor").unwrap(),
        vec!["cursor"]
    );
    assert_eq!(
        storage::list::<serde_json::Value>(&c, "run").unwrap().len(),
        1
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM cardbe_plugin_sources", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn plugin_update_requires_review_for_settings_and_credential_scope_changes() {
    let old = parse_package(br#"{"id":"example","name":"Example","version":"1.0.0","api_version":"1.1.0","storage_schema_version":1,"domains":["api.example.com"],"settings":[{"key":"token","type":"secret","label":"Token","required":true}]}"#).unwrap();
    let mut next = old.clone();
    next.version = "1.1.0".into();
    assert!(!update_requires_review(&old, &next));
    assert!(!credentials_changed(&old, &next));
    next.domains = vec!["new.example.com".into()];
    assert!(update_requires_review(&old, &next) && credentials_changed(&old, &next));
    next = old.clone();
    next.settings[0].key = "new_token".into();
    assert!(update_requires_review(&old, &next) && credentials_changed(&old, &next));
    next = old.clone();
    next.settings[0].required = false;
    assert!(update_requires_review(&old, &next));
    assert!(!credentials_changed(&old, &next));
    let mut legacy = serde_json::to_value(instance()).unwrap();
    legacy.as_object_mut().unwrap().remove("needs_review");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("credentials_need_review");
    let legacy: PluginInstance = serde_json::from_value(legacy).unwrap();
    assert!(!legacy.needs_review && !legacy.credentials_need_review);
    let mut paused = instance();
    paused.needs_review = true;
    assert!(matches!(
        change_enabled(&mut paused, true),
        Err(CommandError::PermissionDenied)
    ));
    assert!(!paused.enabled);
    assert!(change_enabled(&mut paused, false).is_ok());
    assert!(paused.needs_review);
    paused.needs_review = false;
    assert!(change_enabled(&mut paused, true).is_ok());
    assert!(paused.enabled);
}
fn instance() -> PluginInstance {
    PluginInstance {
        id: "a".into(),
        plugin_id: "example".into(),
        name: "Example".into(),
        board_id: 1,
        config: Default::default(),
        secret_fields: vec![],
        allowed_domains: vec![],
        private_network_domains: vec![],
        pending_domain: None,
        enabled: false,
        needs_review: false,
        credentials_need_review: false,
        interval_seconds: 0,
        limits: Default::default(),
        last_run_at: None,
        last_run_status: None,
        next_run_at: None,
        failures: 0,
        last_error: None,
        running: false,
    }
}
#[test]
fn domain_approval_is_persisted_scoped_and_validated_and_declining_pauses() {
    let package: PluginPackage = serde_json::from_value(serde_json::json!({
        "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.1.0",
        "storage_schema_version":1, "domains":["api.example.com"]
    }))
    .unwrap();
    let mut pending = instance();
    pending.enabled = true;
    pending.pending_domain = Some(PendingDomain {
        domain: "api.example.com".into(),
        run_id: "run-1".into(),
        trigger: "manual".into(),
        private_network: false,
    });
    assert_eq!(
        resolve_domain(&mut pending.clone(), &package, "run-1", true, false).unwrap(),
        "manual"
    );
    let mut scheduled = pending.clone();
    scheduled.pending_domain.as_mut().unwrap().trigger = "schedule".into();
    assert_eq!(
        resolve_domain(&mut scheduled, &package, "run-1", true, false).unwrap(),
        "schedule"
    );
    assert!(resolve_domain(&mut pending.clone(), &package, "stale-run", true, false).is_err());
    let mut needs_review = pending.clone();
    let mut future = package.clone();
    future.manifest_version = 3;
    assert!(resolve_domain(&mut pending.clone(), &future, "run-1", true, false).is_err());
    let mut declined = pending.clone();
    resolve_domain(&mut declined, &future, "run-1", false, false).unwrap();
    assert!(!declined.enabled && declined.pending_domain.is_none());
    needs_review.needs_review = true;
    assert!(resolve_domain(&mut needs_review, &package, "run-1", true, false).is_err());
    let mut disabled = pending.clone();
    disabled.enabled = false;
    assert!(resolve_domain(&mut disabled, &package, "run-1", true, false).is_err());
    let mut other_host = pending.clone();
    other_host.pending_domain.as_mut().unwrap().domain = "other.example.com".into();
    assert!(resolve_domain(&mut other_host, &package, "run-1", true, false).is_err());
    let mut full = pending.clone();
    full.allowed_domains = (0..16).map(|i| format!("host{i}.example.com")).collect();
    let mut custom_package = package.clone();
    custom_package.allow_custom_domains = true;
    assert!(resolve_domain(&mut full, &custom_package, "run-1", true, false).is_err());
    let mut rejected = pending.clone();
    resolve_domain(&mut rejected, &package, "run-1", false, false).unwrap();
    assert!(
        !rejected.enabled && rejected.pending_domain.is_none() && rejected.next_run_at.is_none()
    );
    assert!(rejected.allowed_domains.is_empty());
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&connection).unwrap();
    storage::put(&connection, "instance", "pending", &pending).unwrap();
    let mut restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
    resolve_domain(&mut restored, &package, "run-1", true, false).unwrap();
    storage::put(&connection, "instance", "pending", &restored).unwrap();
    let restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
    assert_eq!(restored.allowed_domains, ["api.example.com"]);
    assert!(restored.enabled && restored.pending_domain.is_none());
    assert!(instance().allowed_domains.is_empty());
    assert!(resolve_domain(&mut restored.clone(), &package, "run-1", true, false).is_err());
    let mut legacy = serde_json::to_value(instance()).unwrap();
    legacy.as_object_mut().unwrap().remove("pending_domain");
    assert!(serde_json::from_value::<PluginInstance>(legacy)
        .unwrap()
        .pending_domain
        .is_none());
}
#[test]
fn private_network_access_is_an_option_of_the_host_grant() {
    let package: PluginPackage = serde_json::from_value(serde_json::json!({
        "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.1.0",
        "storage_schema_version":1, "domains":[], "allow_custom_domains":true
    }))
    .unwrap();
    let mut host_request = instance();
    host_request.enabled = true;
    host_request.pending_domain = Some(PendingDomain {
        domain: "jira.example.com:8443".into(),
        run_id: "run-host".into(),
        trigger: "manual".into(),
        private_network: false,
    });
    // One approval can grant the host and its private network access together.
    let mut both = host_request.clone();
    resolve_domain(&mut both, &package, "run-host", true, true).unwrap();
    assert_eq!(both.allowed_domains, ["jira.example.com:8443"]);
    assert_eq!(both.private_network_domains, ["jira.example.com:8443"]);
    let mut public = host_request.clone();
    resolve_domain(&mut public, &package, "run-host", true, false).unwrap();
    assert_eq!(public.allowed_domains, ["jira.example.com:8443"]);
    assert!(public.private_network_domains.is_empty());
    let mut declined = host_request.clone();
    resolve_domain(&mut declined, &package, "run-host", false, true).unwrap();
    assert!(!declined.enabled && declined.allowed_domains.is_empty());
    assert!(declined.private_network_domains.is_empty());

    // A private network request follows an approved host whose DNS answered privately.
    let mut pending = public.clone();
    pending.pending_domain = Some(PendingDomain {
        domain: "jira.example.com:8443".into(),
        run_id: "run-private".into(),
        trigger: "schedule".into(),
        private_network: true,
    });
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&connection).unwrap();
    storage::put(&connection, "instance", "private", &pending).unwrap();
    let mut restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
    assert!(restored.pending_domain.as_ref().unwrap().private_network);
    assert!(resolve_domain(&mut restored.clone(), &package, "stale", true, false).is_err());
    let mut revoked_host = restored.clone();
    revoked_host.allowed_domains.clear();
    assert!(resolve_domain(&mut revoked_host, &package, "run-private", true, false).is_err());
    assert!(revoked_host.private_network_domains.is_empty());
    assert_eq!(
        resolve_domain(&mut restored, &package, "run-private", true, false).unwrap(),
        "schedule"
    );
    assert_eq!(restored.allowed_domains, ["jira.example.com:8443"]);
    assert_eq!(restored.private_network_domains, ["jira.example.com:8443"]);
    assert!(restored.enabled && restored.pending_domain.is_none());
    let mut declined = pending.clone();
    resolve_domain(&mut declined, &package, "run-private", false, false).unwrap();
    assert!(!declined.enabled && declined.private_network_domains.is_empty());
    assert_eq!(declined.allowed_domains, ["jira.example.com:8443"]);

    // Settings grant and revoke it with the host, like other connection permissions.
    let allowed = vec!["jira.example.com:8443".to_owned(), "api.example.com".into()];
    assert_eq!(
        retained_private_network_domains(
            vec![
                "jira.example.com:8443".into(),
                "jira.example.com:8443".into(),
                "api.example.com:443".into(),
                "removed.example.com".into(),
            ],
            &allowed
        ),
        ["jira.example.com:8443", "api.example.com:443"]
    );
    assert!(retained_private_network_domains(allowed.clone(), &[]).is_empty());
    let mut legacy = serde_json::to_value(&pending).unwrap();
    let object = legacy.as_object_mut().unwrap();
    object.remove("private_network_domains");
    object["pending_domain"]
        .as_object_mut()
        .unwrap()
        .remove("private_network");
    let legacy: PluginInstance = serde_json::from_value(legacy).unwrap();
    assert!(legacy.private_network_domains.is_empty());
    assert!(!legacy.pending_domain.unwrap().private_network);
}
#[test]
fn custom_port_pending_requests_and_grants_survive_restart_and_revocation() {
    let package: PluginPackage = serde_json::from_value(serde_json::json!({
        "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.1.0",
        "storage_schema_version":1, "domains":["api.example.com:8443"]
    }))
    .unwrap();
    let mut pending = instance();
    pending.enabled = true;
    pending.pending_domain = Some(PendingDomain {
        domain: "api.example.com:8443".into(),
        run_id: "run-port".into(),
        trigger: "schedule".into(),
        private_network: false,
    });
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&connection).unwrap();
    storage::put(&connection, "instance", "port", &pending).unwrap();
    let mut restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
    assert_eq!(
        restored.pending_domain.as_ref().unwrap().domain,
        "api.example.com:8443"
    );
    assert!(resolve_domain(&mut restored, &package, "stale", true, false).is_err());
    assert_eq!(
        resolve_domain(&mut restored, &package, "run-port", true, false).unwrap(),
        "schedule"
    );
    storage::put(&connection, "instance", "port", &restored).unwrap();
    let mut restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
    assert_eq!(restored.allowed_domains, ["api.example.com:8443"]);
    restored.allowed_domains.clear();
    restored.pending_domain = pending.pending_domain.clone();
    resolve_domain(&mut restored, &package, "run-port", false, false).unwrap();
    assert!(
        !restored.enabled
            && restored.allowed_domains.is_empty()
            && restored.pending_domain.is_none()
    );
}
#[test]
fn manifest_domains_and_select_values_are_unique_and_valid() {
    assert!(valid_domains(&["api.github.com".into()]));
    assert!(valid_domains(&[
        "api.github.com:8443".into(),
        "api.github.com:443".into()
    ]));
    assert!(valid_domains(&["api.github.com:65535".into()]));
    for invalid in [
        vec!["".into()],
        vec!["api.github.com".into(), "api.github.com".into()],
        vec!["bad..example".into()],
        vec!["-bad.example".into()],
        vec!["example.com/path".into()],
        vec!["EXAMPLE.com".into()],
    ] {
        assert!(!valid_domains(&invalid));
    }
    assert!(unique_values(["open", "closed"]));
    assert!(!unique_values(["open", "open"]));
    assert!(!unique_values([""]));
}
#[test]
fn custom_hosts_require_manifest_opt_in_and_valid_explicit_grants() {
    let mut package: PluginPackage = serde_json::from_value(serde_json::json!({
        "id": "fixture", "name": "Fixture", "version": "1.0.0",
        "api_version": "1.1.0", "storage_schema_version": 1,
        "domains": ["api.example.com"]
    }))
    .unwrap();
    assert!(!package.allow_custom_domains);
    assert!(valid_instance_domains(
        &package,
        &["api.example.com".into()]
    ));
    assert!(!valid_instance_domains(
        &package,
        &["custom.example.com".into()]
    ));
    assert!(valid_instance_domains(
        &package,
        &["api.example.com:443".into()]
    ));
    assert!(!valid_instance_domains(
        &package,
        &["api.example.com:8443".into()]
    ));
    package.domains = vec!["api.example.com:8443".into()];
    assert!(valid_instance_domains(
        &package,
        &["api.example.com:8443".into()]
    ));
    assert!(!valid_instance_domains(
        &package,
        &["api.example.com".into()]
    ));
    package.allow_custom_domains = true;
    assert!(valid_instance_domains(
        &package,
        &["api.example.com".into(), "custom.example.com".into()]
    ));
    assert!(valid_instance_domains(&package, &[]));
    for domains in [
        vec!["localhost".into()],
        vec!["a.localhost".into()],
        vec!["127.0.0.1".into()],
        vec!["10.0.0.1".into()],
        vec!["example.com:0".into()],
        vec!["example.com:65536".into()],
        vec!["example.com:0443".into()],
        vec!["example.com".into(), "example.com:443".into()],
        vec!["*.example.com".into()],
        vec!["custom.example.com".into(), "custom.example.com".into()],
        (0..17).map(|i| format!("host{i}.example.com")).collect(),
    ] {
        assert!(!valid_instance_domains(&package, &domains));
    }
}
#[test]
fn explicit_credential_rules_allow_multiple_secrets_and_require_update_review() {
    let mut manifest = serde_json::json!({
        "id":"scoped", "name":"Scoped", "version":"1.0.0", "api_version":"1.1.0", "storage_schema_version":1,
        "domains":["fixed.example.com"], "allow_custom_domains":true,
        "settings":[{"key":"base_url","type":"text","label":"URL"},{"key":"token","type":"secret","label":"Token"},{"key":"admin_token","type":"secret","label":"Admin token"}],
        "http_credentials":[
            {"destination":{"type":"base_url","setting":"base_url"},"path_prefix":"/tasks","bearer_secret":"token"},
            {"destination":{"type":"base_url","setting":"base_url"},"path_prefix":"/admin","bearer_secret":"admin_token"},
            {"destination":{"type":"domain","domain":"fixed.example.com"},"path_prefix":"/public"}
        ]
    });
    let old = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut header = manifest.clone();
    header["http_credentials"][0]
        .as_object_mut()
        .unwrap()
        .remove("bearer_secret");
    header["http_credentials"][0]["auth"] =
        serde_json::json!({"type":"header", "name":"X-API-Key", "secret":"token"});
    assert!(parse_package(&serde_json::to_vec(&header).unwrap()).is_err());
    header["manifest_version"] = 2.into();
    let keyed = parse_package(&serde_json::to_vec(&header).unwrap()).unwrap();
    assert!(update_requires_review(&old, &keyed) && credentials_changed(&old, &keyed));
    assert_eq!(
        parse_package(&serde_json::to_vec(&keyed).unwrap())
            .unwrap()
            .http_credentials,
        keyed.http_credentials
    );
    let mut renamed = header.clone();
    renamed["http_credentials"][0]["auth"]["name"] = "Api-Key".into();
    let renamed = parse_package(&serde_json::to_vec(&renamed).unwrap()).unwrap();
    assert!(update_requires_review(&keyed, &renamed) && credentials_changed(&keyed, &renamed));
    for name in [
        "Authorization",
        "hOsT",
        "Cookie",
        "Content-Length",
        "Connection",
        "Proxy-Authorization",
        "Transfer-Encoding",
        "Sec-Fetch-Site",
        "Content-Type",
        "",
        "X-API-Key\r\nOther",
    ] {
        let mut invalid = header.clone();
        invalid["http_credentials"][0]["auth"]["name"] = name.into();
        assert!(
            parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err(),
            "{name}"
        );
    }
    for legacy in [serde_json::Value::Null, serde_json::json!("token")] {
        let mut invalid = header.clone();
        invalid["http_credentials"][0]["bearer_secret"] = legacy;
        assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    header["http_credentials"][0]["auth"] = serde_json::json!({"type":"bearer", "secret":"token"});
    assert!(parse_package(&serde_json::to_vec(&header).unwrap()).is_ok());
    header["http_credentials"][0]["auth"]["secret"] = "missing".into();
    assert!(parse_package(&serde_json::to_vec(&header).unwrap()).is_err());
    let mut defaults = manifest.clone();
    defaults["allow_custom_domains"] = serde_json::json!(false);
    for url in [
        "https://custom.example.com/api",
        "https://fixed.example.com:8443/api",
        "https://fixed.example.com/api//",
        "https://fixed.example.com/a/../api",
    ] {
        defaults["settings"][0]["default"] = serde_json::json!(url);
        assert!(
            parse_package(&serde_json::to_vec(&defaults).unwrap()).is_err(),
            "{url}"
        );
    }
    for url in [
        "",
        "https://fixed.example.com/",
        "https://fixed.example.com:443/api/",
    ] {
        defaults["settings"][0]["default"] = serde_json::json!(url);
        assert!(
            parse_package(&serde_json::to_vec(&defaults).unwrap()).is_ok(),
            "{url}"
        );
    }
    defaults["allow_custom_domains"] = serde_json::json!(true);
    defaults["settings"][0]["default"] = serde_json::json!("https://custom.example.com:8443/api/");
    assert!(parse_package(&serde_json::to_vec(&defaults).unwrap()).is_ok());
    let c = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&c).unwrap();
    storage::put(&c, "package", &old.id, &old).unwrap();
    let loaded = storage::list::<PluginPackage>(&c, "package")
        .unwrap()
        .remove(0);
    assert_eq!(loaded.http_credentials, old.http_credentials);
    let mut next = old.clone();
    next.http_credentials.as_mut().unwrap()[0].path_prefix = "/other".into();
    assert!(update_requires_review(&old, &next) && credentials_changed(&old, &next));
    next = old.clone();
    next.settings[2].key = "retired".into();
    assert!(credentials_changed(&old, &next));
    for key in ["bearer_secret", "path_prefix"] {
        let mut invalid = manifest.clone();
        invalid["http_credentials"][0][key] = serde_json::json!("missing");
        assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut invalid = manifest.clone();
    invalid["http_credentials"][0]["token"] = serde_json::json!("token");
    assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
    invalid = manifest.clone();
    invalid["error_codes"] = serde_json::json!(["bad code"]);
    assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
    manifest.as_object_mut().unwrap().remove("http_credentials");
    assert!(parse_package(&serde_json::to_vec(&manifest).unwrap()).is_err());
    manifest["http_credentials"] = serde_json::json!([]);
    let explicit_empty = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(explicit_empty.http_credentials, Some(vec![]));
}
#[test]
fn credential_review_never_reactivates_secrets_that_were_not_reentered() {
    let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"fixture", "name":"Fixture", "version":"1.0.0", "api_version":"1.1.0", "storage_schema_version":1,
            "http_credentials":[], "settings":[{"key":"token","type":"secret","label":"Token"},{"key":"other_token","type":"secret","label":"Other"}]
        })).unwrap();
    let mut previous = instance();
    previous.secret_fields = vec!["token".into(), "other_token".into()];
    previous.credentials_need_review = true;
    let secrets = BTreeMap::from([("token".into(), "fixture-new-token".into())]);
    assert_eq!(
        updated_secret_fields(&package, Some(&previous), &secrets).unwrap(),
        vec!["token"]
    );
    assert!(
        updated_secret_fields(&package, Some(&previous), &BTreeMap::new())
            .unwrap()
            .is_empty()
    );
    previous.credentials_need_review = false;
    assert_eq!(
        updated_secret_fields(&package, Some(&previous), &secrets).unwrap(),
        vec!["other_token", "token"]
    );
    for invalid in [
        BTreeMap::from([("missing".into(), "fixture".into())]),
        BTreeMap::from([("token".into(), "x".repeat(8193))]),
        BTreeMap::from([("token".into(), "bad\r\nX-Injected: yes".into())]),
        BTreeMap::from([("token".into(), "bad\0".into())]),
    ] {
        assert!(updated_secret_fields(&package, Some(&previous), &invalid).is_err());
    }
}
#[test]
fn package_ids_cannot_alias_windows_paths() {
    for invalid in [
        "..", "...", "foo.", "CON", "con", "nul", "com1", "lpt9", "a/b", "a\\b", "../a",
    ] {
        assert!(!valid_id(invalid), "{invalid}");
    }
    assert!(valid_id("example-importer"));
}
#[test]
fn uninstall_detaches_sources_and_runs_atomically() {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    storage::initialize(&c).unwrap();
    let i = instance();
    storage::put(&c, "instance", &i.id, &i).unwrap();
    storage::put(&c, "cursor", &i.id, &Some("cursor")).unwrap();
    c.execute(
        "INSERT INTO cardbe_plugin_sources VALUES('a','source',1,2,'{}')",
        [],
    )
    .unwrap();
    storage::put(&c, "run", "run-1", &serde_json::json!({"instance_id":"a"})).unwrap();
    c.execute_batch("CREATE TRIGGER refuse_detach BEFORE DELETE ON cardbe_plugin_sources BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert!(remove_locked(&c, std::slice::from_ref(&i), &BTreeMap::new(), None).is_err());
    let count: i64 = c
        .query_row("SELECT count(*) FROM cardbe_plugin_data", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 3);
    c.execute_batch("DROP TRIGGER refuse_detach;").unwrap();
    remove_locked(&c, &[i], &BTreeMap::new(), None).unwrap();
    let count: i64 = c
        .query_row("SELECT count(*) FROM cardbe_plugin_data", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn reinstall_clears_files_left_by_an_uninstalled_package_inside_the_root() {
    let base = std::env::temp_dir().join(format!("cardbe-orphan-test-{}", id()));
    let root = base.join("plugins");
    let destination = root.join("example");
    std::fs::create_dir_all(destination.join("revision")).unwrap();
    std::fs::write(destination.join("revision").join("component.wasm"), b"old").unwrap();
    clear_orphaned_package(&root, &destination).unwrap();
    assert!(!destination.exists());
    clear_orphaned_package(&root, &destination).unwrap();
    std::fs::write(&destination, b"file").unwrap();
    assert!(clear_orphaned_package(&root, &destination).is_err());
    assert!(destination.exists());
    std::fs::remove_file(&destination).unwrap();
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    assert!(clear_orphaned_package(&root, &outside).is_err());
    assert!(outside.exists());
    std::fs::remove_dir_all(base).unwrap();
}
