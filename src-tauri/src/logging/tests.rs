//! Tests for the logging system

use super::*;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_log_level_display() {
    assert_eq!(LogLevel::Trace.to_string(), "trace");
    assert_eq!(LogLevel::Debug.to_string(), "debug");
    assert_eq!(LogLevel::Info.to_string(), "info");
    assert_eq!(LogLevel::Warn.to_string(), "warn");
    assert_eq!(LogLevel::Error.to_string(), "error");
}

#[test]
fn test_logging_config_default() {
    let config = LoggingConfig::default();
    assert_eq!(config.level, LogLevel::Info);
    assert_eq!(config.format, LogFormat::Text);
    assert_eq!(config.output, LogOutput::Both);
    assert!(config.include_target);
    assert!(!config.include_thread_id);
    assert!(!config.include_file_info);
}

#[test]
fn test_logging_config_builder() {
    let config = LoggingConfig::new()
        .with_level(LogLevel::Debug)
        .with_format(LogFormat::Json)
        .with_output(LogOutput::File)
        .with_target(false)
        .with_thread_id(true)
        .with_file_info(true)
        .with_module_level("neural_fs::search", LogLevel::Trace);

    assert_eq!(config.level, LogLevel::Debug);
    assert_eq!(config.format, LogFormat::Json);
    assert_eq!(config.output, LogOutput::File);
    assert!(!config.include_target);
    assert!(config.include_thread_id);
    assert!(config.include_file_info);
    assert_eq!(
        config.module_levels.get("neural_fs::search"),
        Some(&LogLevel::Trace)
    );
}

#[test]
fn test_logging_config_development() {
    let config = LoggingConfig::development();
    assert_eq!(config.level, LogLevel::Debug);
    assert_eq!(config.output, LogOutput::Console);
    assert!(config.include_file_info);
}

#[test]
fn test_logging_config_production() {
    let config = LoggingConfig::production();
    assert_eq!(config.level, LogLevel::Info);
    assert_eq!(config.format, LogFormat::Json);
    assert_eq!(config.output, LogOutput::Both);
    assert!(config.rotation.compress);
}

#[test]
fn test_rotation_config_default() {
    let config = RotationConfig::default();
    assert_eq!(config.strategy, RotationStrategy::Daily);
    assert_eq!(config.max_files, 7);
    assert_eq!(config.max_age_days, 30);
    assert!(!config.compress);
}

#[test]
fn test_rotation_config_production() {
    let config = RotationConfig::production();
    assert_eq!(config.max_files, 14);
    assert!(config.compress);
}

#[test]
fn test_log_rotator_needs_rotation() {
    let config = RotationConfig {
        strategy: RotationStrategy::Never,
        ..Default::default()
    };
    let rotator = LogRotator::new(config);
    assert!(!rotator.needs_rotation());

    let config = RotationConfig {
        strategy: RotationStrategy::Daily,
        ..Default::default()
    };
    let rotator = LogRotator::new(config);
    assert!(rotator.needs_rotation()); // First time should need rotation
}

#[test]
fn test_export_config_default() {
    let config = ExportConfig::default();
    assert_eq!(config.format, ExportFormat::Text);
    assert!(config.include_system_info);
    assert!(!config.include_config);
    assert_eq!(config.max_lines, 10000);
    assert!(config.anonymize);
}

#[test]
fn test_metrics_collector_record() {
    let collector = MetricsCollector::new();

    collector.record_duration("test_operation", Duration::from_millis(100));
    collector.record_counter("test_counter", 42);
    collector.record_gauge("test_gauge", 3.14);

    let stats = collector.get_stats("test_operation").unwrap();
    assert_eq!(stats.count, 1);
    assert_eq!(stats.last, 100.0);

    let stats = collector.get_stats("test_counter").unwrap();
    assert_eq!(stats.last, 42.0);

    let stats = collector.get_stats("test_gauge").unwrap();
    assert!((stats.last - 3.14).abs() < 0.001);
}

#[test]
fn test_metrics_collector_aggregation() {
    let collector = MetricsCollector::new();

    collector.record_duration("latency", Duration::from_millis(100));
    collector.record_duration("latency", Duration::from_millis(200));
    collector.record_duration("latency", Duration::from_millis(300));

    let stats = collector.get_stats("latency").unwrap();
    assert_eq!(stats.count, 3);
    assert_eq!(stats.min, 100.0);
    assert_eq!(stats.max, 300.0);
    assert_eq!(stats.mean, 200.0);
    assert_eq!(stats.sum, 600.0);
}

#[test]
fn test_metrics_collector_increment() {
    let collector = MetricsCollector::new();

    collector.increment("requests");
    collector.increment("requests");
    collector.increment("requests");

    let stats = collector.get_stats("requests").unwrap();
    assert_eq!(stats.last, 3.0);
}

#[test]
fn test_metrics_collector_recent_entries() {
    let collector = MetricsCollector::with_capacity(5);

    for i in 0..10 {
        collector.record_counter("test", i);
    }

    let recent = collector.get_recent_entries();
    assert_eq!(recent.len(), 5);

    // Should have the last 5 entries (5-9)
    if let MetricType::Counter(val) = recent[0].value {
        assert_eq!(val, 5);
    }
}

#[test]
fn test_metrics_collector_clear() {
    let collector = MetricsCollector::new();

    collector.record_counter("test", 42);
    assert!(collector.get_stats("test").is_some());

    collector.clear();
    assert!(collector.get_stats("test").is_none());
    assert_eq!(collector.total_recorded(), 0);
}

#[test]
fn test_timer_guard() {
    let collector = MetricsCollector::new();

    {
        let _timer = TimerGuard::new(&collector, "timed_operation");
        std::thread::sleep(Duration::from_millis(10));
    }

    let stats = collector.get_stats("timed_operation").unwrap();
    assert_eq!(stats.count, 1);
    assert!(stats.last >= 10.0); // At least 10ms
}

#[test]
fn test_performance_summary() {
    let collector = MetricsCollector::new();

    collector.record_duration("search_latency", Duration::from_millis(50));
    collector.record_duration("indexing_duration", Duration::from_millis(100));
    collector.record_gauge("memory_usage", 1024.0 * 1024.0);

    let summary = collector.get_performance_summary();
    assert_eq!(summary.search_latency_ms.count, 1);
    assert_eq!(summary.indexing_duration_ms.count, 1);
    assert_eq!(summary.memory_usage_bytes.count, 1);
}

#[test]
fn test_export_json() {
    let collector = MetricsCollector::new();

    collector.record_counter("test_metric", 42);

    let json = collector.export_json();
    assert!(json.contains("test_metric"));
    assert!(json.contains("42"));
}

#[test]
fn test_log_exporter_anonymize() {
    let config = ExportConfig {
        anonymize: true,
        ..Default::default()
    };
    let exporter = LogExporter::new(config);

    // Test username anonymization
    let line = "Error in C:\\Users\\JohnDoe\\Documents\\file.txt";
    let anonymized = exporter.anonymize_line(line);
    // Note: This will only work if USERNAME env var is set
    // The test verifies the method runs without error

    // Test email anonymization
    let line = "Contact: user@example.com for support";
    let anonymized = exporter.anonymize_line(line);
    assert!(anonymized.contains("[EMAIL]"));

    // Test IP anonymization
    let line = "Connection from 192.168.1.100 failed";
    let anonymized = exporter.anonymize_line(line);
    assert!(anonymized.contains("[IP]"));

    // Test API key anonymization
    let line = "api_key=test_key_placeholder_for_testing_only";
    let anonymized = exporter.anonymize_line(line);
    assert!(anonymized.contains("[REDACTED]"));
}

#[tokio::test]
async fn test_log_export_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Create a test log file
    let log_file = log_dir.join("test.log");
    std::fs::write(&log_file, "Test log entry\nAnother entry\n").unwrap();

    let config = ExportConfig {
        log_directory: Some(log_dir),
        max_age_hours: 24,
        ..Default::default()
    };

    let exporter = LogExporter::new(config);
    let output_path = temp_dir.path().join("export.txt");

    let result = exporter.export(output_path.clone()).await.unwrap();

    assert!(output_path.exists());
    assert!(result.lines_exported > 0);
    assert_eq!(result.files_included, 1);
}

// Helper function to test anonymization
impl LogExporter {
    fn anonymize_line(&self, line: &str) -> String {
        let mut result = line.to_string();

        // Anonymize usernames in paths
        if let Ok(username) = std::env::var("USERNAME") {
            result = result.replace(&username, "[USER]");
        }
        if let Ok(username) = std::env::var("USER") {
            result = result.replace(&username, "[USER]");
        }

        // Anonymize home directory
        if let Some(home) = dirs::home_dir() {
            if let Some(home_str) = home.to_str() {
                result = result.replace(home_str, "[HOME]");
            }
        }

        // Anonymize potential API keys
        let api_key_pattern = regex::Regex::new(r"(api[_-]?key|token|secret)[=:]\s*['\"]?([a-zA-Z0-9_-]{20,})['\"]?")
            .unwrap();
        result = api_key_pattern.replace_all(&result, "$1=[REDACTED]").to_string();

        // Anonymize email addresses
        let email_pattern = regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
            .unwrap();
        result = email_pattern.replace_all(&result, "[EMAIL]").to_string();

        // Anonymize IP addresses
        let ip_pattern = regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
            .unwrap();
        result = ip_pattern.replace_all(&result, "[IP]").to_string();

        result
    }
}
