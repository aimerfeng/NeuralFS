//! Tests for the telemetry system

use super::*;
use tempfile::TempDir;

#[test]
fn test_telemetry_config_default() {
    let config = TelemetryConfig::default();
    assert!(!config.enabled); // Disabled by default
    assert!(config.collect_performance);
    assert!(config.collect_feature_usage);
    assert!(config.collect_errors);
    assert_eq!(config.batch_size, 50);
}

#[test]
fn test_telemetry_config_builder() {
    let config = TelemetryConfig::new()
        .with_enabled(true)
        .with_endpoint("https://telemetry.example.com")
        .with_api_key("test-key")
        .with_batch_size(100)
        .with_sampling_rate(0.5);

    assert!(config.enabled);
    assert_eq!(config.endpoint.url, "https://telemetry.example.com");
    assert_eq!(config.endpoint.api_key, Some("test-key".to_string()));
    assert_eq!(config.batch_size, 100);
    assert!((config.performance_sampling_rate - 0.5).abs() < 0.001);
}

#[test]
fn test_consent_status_default() {
    assert_eq!(ConsentStatus::default(), ConsentStatus::NotAsked);
}

#[test]
fn test_consent_manager_new() {
    let temp_dir = TempDir::new().unwrap();
    let consent_file = temp_dir.path().join("consent.json");

    let manager = ConsentManager::new(consent_file).unwrap();
    assert_eq!(manager.status(), ConsentStatus::NotAsked);
    assert!(!manager.has_consent());
}

#[test]
fn test_consent_manager_grant() {
    let temp_dir = TempDir::new().unwrap();
    let consent_file = temp_dir.path().join("consent.json");

    let manager = ConsentManager::new(consent_file.clone()).unwrap();
    manager.grant_consent().unwrap();

    assert_eq!(manager.status(), ConsentStatus::Granted);
    assert!(manager.has_consent());

    // Verify persistence
    let manager2 = ConsentManager::new(consent_file).unwrap();
    assert_eq!(manager2.status(), ConsentStatus::Granted);
}

#[test]
fn test_consent_manager_deny() {
    let temp_dir = TempDir::new().unwrap();
    let consent_file = temp_dir.path().join("consent.json");

    let manager = ConsentManager::new(consent_file).unwrap();
    manager.deny_consent().unwrap();

    assert_eq!(manager.status(), ConsentStatus::Denied);
    assert!(!manager.has_consent());
}

#[test]
fn test_consent_manager_revoke() {
    let temp_dir = TempDir::new().unwrap();
    let consent_file = temp_dir.path().join("consent.json");

    let manager = ConsentManager::new(consent_file).unwrap();
    manager.grant_consent().unwrap();
    assert!(manager.has_consent());

    manager.revoke_consent().unwrap();
    assert_eq!(manager.status(), ConsentStatus::Revoked);
    assert!(!manager.has_consent());
}

#[test]
fn test_consent_history() {
    let temp_dir = TempDir::new().unwrap();
    let consent_file = temp_dir.path().join("consent.json");

    let manager = ConsentManager::new(consent_file).unwrap();
    manager.grant_consent().unwrap();
    manager.revoke_consent().unwrap();

    let history = manager.get_history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status, ConsentStatus::NotAsked);
    assert_eq!(history[1].status, ConsentStatus::Granted);
}

#[test]
fn test_telemetry_event_creation() {
    let event = TelemetryEvent::feature_usage(
        "install-123".to_string(),
        "session-456".to_string(),
        "search",
        "execute",
    );

    assert_eq!(event.event_type, EventType::FeatureUsage);
    assert_eq!(event.installation_id, "install-123");
    assert_eq!(event.session_id, "session-456");

    if let EventData::FeatureUsage(usage) = event.data {
        assert_eq!(usage.feature_name, "search");
        assert_eq!(usage.action, "execute");
    } else {
        panic!("Expected FeatureUsage data");
    }
}

#[test]
fn test_telemetry_event_performance() {
    let event = TelemetryEvent::performance(
        "install-123".to_string(),
        "session-456".to_string(),
        "search_query",
        150,
        true,
    );

    assert_eq!(event.event_type, EventType::Performance);

    if let EventData::Performance(perf) = event.data {
        assert_eq!(perf.operation, "search_query");
        assert_eq!(perf.duration_ms, 150);
        assert!(perf.success);
    } else {
        panic!("Expected Performance data");
    }
}

#[test]
fn test_telemetry_event_error() {
    let event = TelemetryEvent::error(
        "install-123".to_string(),
        "session-456".to_string(),
        "database_error",
        Some("E001".to_string()),
    );

    assert_eq!(event.event_type, EventType::Error);

    if let EventData::Error(err) = event.data {
        assert_eq!(err.error_type, "database_error");
        assert_eq!(err.error_code, Some("E001".to_string()));
    } else {
        panic!("Expected Error data");
    }
}

#[test]
fn test_telemetry_collector_add_event() {
    let config = TelemetryConfig::default();
    let mut collector = TelemetryCollector::new(config);

    let event = TelemetryEvent::feature_usage(
        "install-123".to_string(),
        "session-456".to_string(),
        "search",
        "execute",
    );

    collector.add_event(event);
    assert_eq!(collector.event_count(), 1);
    assert_eq!(collector.pending_count(), 1);
}

#[test]
fn test_telemetry_collector_batch_creation() {
    let config = TelemetryConfig::new().with_batch_size(5);
    let mut collector = TelemetryCollector::new(config);

    // Add 5 events to trigger batch creation
    for i in 0..5 {
        let event = TelemetryEvent::feature_usage(
            "install-123".to_string(),
            "session-456".to_string(),
            format!("feature_{}", i),
            "use",
        );
        collector.add_event(event);
    }

    assert_eq!(collector.event_count(), 5);
}

#[test]
fn test_telemetry_collector_clear() {
    let config = TelemetryConfig::default();
    let mut collector = TelemetryCollector::new(config);

    let event = TelemetryEvent::feature_usage(
        "install-123".to_string(),
        "session-456".to_string(),
        "search",
        "execute",
    );

    collector.add_event(event);
    assert_eq!(collector.event_count(), 1);

    collector.clear();
    assert_eq!(collector.event_count(), 0);
    assert_eq!(collector.pending_count(), 0);
}

#[test]
fn test_telemetry_batch() {
    let events = vec![
        TelemetryEvent::feature_usage(
            "install-123".to_string(),
            "session-456".to_string(),
            "search",
            "execute",
        ),
        TelemetryEvent::performance(
            "install-123".to_string(),
            "session-456".to_string(),
            "query",
            100,
            true,
        ),
    ];

    let batch = TelemetryBatch::new(events, "0.1.0".to_string(), "windows".to_string());

    assert!(!batch.is_empty());
    assert_eq!(batch.len(), 2);
    assert_eq!(batch.app_version, "0.1.0");
    assert_eq!(batch.platform, "windows");
}

#[test]
fn test_platform_info_default() {
    let info = PlatformInfo::default();
    assert!(!info.os.is_empty());
    assert!(!info.arch.is_empty());
    assert!(!info.app_version.is_empty());
}

#[test]
fn test_consent_dialog_info() {
    let info = get_consent_dialog_info();
    assert!(!info.title.is_empty());
    assert!(!info.message.is_empty());
    assert!(!info.data_collected.is_empty());
    assert!(!info.data_not_collected.is_empty());
}

#[tokio::test]
async fn test_telemetry_system_disabled() {
    let temp_dir = TempDir::new().unwrap();
    let config = TelemetryConfig::new()
        .with_enabled(false)
        .with_data_directory(temp_dir.path().to_path_buf());

    let system = TelemetrySystem::new(config).unwrap();
    assert!(!system.is_enabled());

    // Recording should succeed silently when disabled
    let result = system.record_feature_usage("test", "action");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_telemetry_system_no_consent() {
    let temp_dir = TempDir::new().unwrap();
    let config = TelemetryConfig::new()
        .with_enabled(true)
        .with_data_directory(temp_dir.path().to_path_buf());

    let system = TelemetrySystem::new(config).unwrap();

    // Should not be enabled without consent
    assert!(!system.is_enabled());
}

#[tokio::test]
async fn test_telemetry_system_with_consent() {
    let temp_dir = TempDir::new().unwrap();
    let config = TelemetryConfig::new()
        .with_enabled(true)
        .with_data_directory(temp_dir.path().to_path_buf());

    let system = TelemetrySystem::new(config).unwrap();

    // Grant consent
    system.consent_manager().grant_consent().unwrap();

    // Now should be enabled
    assert!(system.is_enabled());

    // Recording should work
    let result = system.record_feature_usage("test", "action");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_telemetry_system_disable_and_clear() {
    let temp_dir = TempDir::new().unwrap();
    let config = TelemetryConfig::new()
        .with_enabled(true)
        .with_data_directory(temp_dir.path().to_path_buf());

    let system = TelemetrySystem::new(config).unwrap();
    system.consent_manager().grant_consent().unwrap();
    assert!(system.is_enabled());

    // Record some events
    system.record_feature_usage("test", "action").unwrap();

    // Disable and clear
    system.disable_and_clear().unwrap();

    // Should no longer be enabled
    assert!(!system.is_enabled());
    assert_eq!(system.consent_manager().status(), ConsentStatus::Revoked);
}

#[test]
fn test_session_stats() {
    let temp_dir = TempDir::new().unwrap();
    let config = TelemetryConfig::new()
        .with_data_directory(temp_dir.path().to_path_buf());

    let system = TelemetrySystem::new(config).unwrap();
    let stats = system.get_session_stats();

    assert!(!stats.session_id.is_empty());
    assert_eq!(stats.events_recorded, 0);
    assert_eq!(stats.events_sent, 0);
    assert!(!stats.is_enabled);
    assert!(!stats.has_consent);
}
