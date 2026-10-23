use super::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct ReleaseServer {
    repository: Url,
    api: String,
    calls: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ReleaseServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn release_server(api_status: u16, page_status: u16, asset_status: u16) -> ReleaseServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let repository = Url::parse(&format!("http://{address}/Nowaterisenough/HomeNet/")).unwrap();
    let api = format!("http://{address}/api");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let log = calls.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0_u8; 8192];
            let Ok(Ok(length)) =
                tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer)).await
            else {
                continue;
            };
            let request = String::from_utf8_lossy(&buffer[..length]);
            let first = request.lines().next().unwrap_or_default().to_string();
            let path = first.split_whitespace().nth(1).unwrap_or_default();
            let (status, headers, body) = if path == "/api" {
                (
                    api_status,
                    "Content-Type: application/json\r\nX-RateLimit-Remaining: 0\r\n",
                    r#"{"tag_name":"v1.2.3","assets":[]}"#,
                )
            } else if path.ends_with("/releases/latest") {
                (
                    302,
                    "Location: /Nowaterisenough/HomeNet/releases/tag/v1.2.3\r\n",
                    "",
                )
            } else if path.ends_with("/releases/tag/v1.2.3") {
                (page_status, "", "")
            } else if path.contains("/releases/download/v1.2.3/") {
                (asset_status, "", "")
            } else {
                (404, "", "")
            };
            log.lock().unwrap().push(first);
            let response = format!("HTTP/1.1 {status} Fixture\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    ReleaseServer {
        repository,
        api,
        calls,
        task,
    }
}

#[tokio::test]
async fn update_api_rate_limits_fall_back_to_official_version_and_verify_assets() {
    for status in [403, 429] {
        let server = release_server(status, 200, 200).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let release = fetch_github_release_from(&client, &server.api, &server.repository)
            .await
            .unwrap();
        assert_eq!(release.tag_name, "v1.2.3");
        assert!(release.asset_urls_inferred);
        let asset =
            select_github_update_asset(&release, UpdateAssetKind::MacosArm64AppZip).unwrap();
        assert_eq!(asset.name, "HomeNet_v1.2.3_macos-arm64-app.zip");
        verify_inferred_update_asset(&client, &release, asset)
            .await
            .unwrap();
        let calls = server.calls.lock().unwrap();
        assert!(calls
            .iter()
            .any(|line| line.starts_with("HEAD /Nowaterisenough/HomeNet/releases/latest")));
        assert!(calls
            .iter()
            .any(|line| line.starts_with("HEAD /Nowaterisenough/HomeNet/releases/download/")));
        assert!(!calls
            .iter()
            .any(|line| line.starts_with("GET /Nowaterisenough/HomeNet/releases/download/")));
    }
}

#[tokio::test]
async fn unavailable_fallback_is_an_error_not_an_up_to_date_result() {
    let server = release_server(403, 503, 200).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let error = fetch_github_release_from(&client, &server.api, &server.repository)
        .await
        .unwrap_err();
    assert!(error.contains("额度已用尽"));
    assert!(error.contains("503"));
}

#[tokio::test]
async fn missing_inferred_asset_is_not_offered_for_installation() {
    let server = release_server(403, 200, 404).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let release = fetch_github_release_from(&client, &server.api, &server.repository)
        .await
        .unwrap();
    let asset = select_github_update_asset(&release, UpdateAssetKind::WindowsX64Setup).unwrap();
    assert!(verify_inferred_update_asset(&client, &release, asset)
        .await
        .unwrap_err()
        .contains("404"));
}

#[tokio::test]
async fn valid_api_metadata_does_not_request_a_fallback() {
    let server = release_server(200, 503, 404).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let release = fetch_github_release_from(&client, &server.api, &server.repository)
        .await
        .unwrap();
    assert!(!release.asset_urls_inferred);
    assert_eq!(server.calls.lock().unwrap().len(), 1);
}

#[test]
fn fallback_rejects_non_release_or_foreign_redirects() {
    let repository = Url::parse(GITHUB_REPOSITORY_URL).unwrap();
    for value in [
        "https://example.test/Nowaterisenough/HomeNet/releases/tag/v1.2.3",
        "https://github.com/other/HomeNet/releases/tag/v1.2.3",
        "https://github.com/Nowaterisenough/HomeNet/releases/latest",
        "https://github.com/Nowaterisenough/HomeNet/releases/tag/not-a-version",
        "https://github.com/Nowaterisenough/HomeNet/releases/tag/v1.2.3/extra",
    ] {
        assert!(
            release_from_official_page(&Url::parse(value).unwrap(), &repository).is_err(),
            "{value}"
        );
    }
    assert!(validate_github_asset_url(
        "http://github.com/Nowaterisenough/HomeNet/releases/download/v1.2.3/file.exe"
    )
    .is_err());
    assert!(validate_github_asset_url("https://example.test/update.exe").is_err());
    assert!(validate_github_asset_url(
        "https://github.com/other/repo/releases/download/v1.2.3/file.exe"
    )
    .is_err());
}

fn binding(sub: &str, record_type: &str) -> DeviceDdnsConfig {
    DeviceDdnsConfig {
        enabled: true,
        access_key_id: "test-key".into(),
        access_key_secret: "test-secret".into(),
        domain: "example.test".into(),
        sub_domain: sub.into(),
        record_type: record_type.into(),
        device_id: "test-device".into(),
        device_mac: "02:aa:bb:cc:dd:01".into(),
        ..DeviceDdnsConfig::default()
    }
}

#[test]
fn creating_multiple_records_on_one_device_does_not_replace_existing_bindings() {
    let mut config = AppConfig::default();
    create_device_binding(&mut config, binding("", "AAAA")).unwrap();
    create_device_binding(&mut config, binding("game", "AAAA")).unwrap();
    create_device_binding(&mut config, binding("game", "A")).unwrap();
    assert_eq!(active_device_ddns_configs(&config).len(), 3);
    let original = config.device_ddns_configs.clone();
    assert!(create_device_binding(&mut config, binding("GAME", "AAAA")).is_err());
    assert_eq!(config.device_ddns_configs, original);
}

#[test]
fn editing_and_deleting_select_only_the_requested_record() {
    let mut config = AppConfig::default();
    let root = create_device_binding(&mut config, binding("", "AAAA")).unwrap();
    let game = create_device_binding(&mut config, binding("game", "AAAA")).unwrap();
    let game_key = crate::config::device_ddns_record_key(&game).unwrap();
    assert!(save_device_binding(&mut config, binding("", "AAAA"), Some(&game_key)).is_err());
    save_device_binding(&mut config, binding("cloud", "AAAA"), Some(&game_key)).unwrap();
    assert_eq!(active_device_ddns_configs(&config).len(), 2);
    let cloud_key = crate::config::device_ddns_record_key(&binding("cloud", "AAAA")).unwrap();
    assert!(delete_device_ddns_config_from_config(
        &mut config,
        &game.device_id,
        &game.device_mac,
        Some(&cloud_key)
    ));
    assert_eq!(active_device_ddns_configs(&config), vec![root]);
}

#[test]
fn manual_creation_normalizes_mac_and_drops_stale_runtime_fields() {
    let mut draft = binding("manual", "AAAA");
    draft.device_id.clear();
    draft.device_mac = "02-AA-BB-CC-DD-02".into();
    draft.last_online = true;
    draft.last_result = "old result".into();
    draft.last_update_time = "old time".into();
    let saved = create_device_binding(&mut AppConfig::default(), draft).unwrap();
    assert_eq!(saved.device_id, "mac-02-aa-bb-cc-dd-02");
    assert_eq!(saved.device_mac, "02:aa:bb:cc:dd:02");
    assert!(!saved.last_online);
    assert!(saved.last_result.is_empty() && saved.last_update_time.is_empty());
}

#[test]
fn stale_edits_cannot_take_over_a_record_reassigned_to_another_device() {
    let old = binding("game", "AAAA");
    let mut current = old.clone();
    current.device_id = "other-device".into();
    current.device_mac = "02:aa:bb:cc:dd:02".into();
    let original_key = crate::config::device_ddns_record_key(&old).unwrap();
    let mut config = AppConfig {
        device_ddns_configs: vec![current.clone()],
        ..AppConfig::default()
    };
    assert!(save_device_binding(&mut config, old, Some(&original_key)).is_err());
    assert_eq!(config.device_ddns_configs, vec![current]);
}

#[test]
fn creation_validates_device_identity_and_record_type_before_saving() {
    for mac in [
        "",
        "not-a-mac",
        "00:00:00:00:00:00",
        "ff:ff:ff:ff:ff:ff",
        "01:00:5e:00:00:01",
    ] {
        let mut draft = binding("manual", "AAAA");
        draft.device_mac = mac.into();
        let mut config = AppConfig::default();
        assert!(create_device_binding(&mut config, draft).is_err());
        assert!(config.device_ddns_configs.is_empty());
    }
    assert!(create_device_binding(&mut AppConfig::default(), binding("manual", "MX")).is_err());
}
