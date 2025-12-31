//! Cloud Inference Bridge for NeuralFS
//!
//! This module provides cloud API integration:
//! - Support for GPT-4o-mini and Claude Haiku
//! - Rate limiting to prevent API abuse
//! - Cost tracking with database persistence
//! - Retry logic with exponential backoff
//!
//! **Validates: Requirements 11.6, 11.7**

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Utc};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use super::anonymizer::DataAnonymizer;
use super::error::{InferenceError, InferenceResult};
use super::types::{CloudModelType, CloudUnderstanding, InferenceRequest};

/// System prompt for cloud inference
const SYSTEM_PROMPT: &str = r#"你是一个文件搜索助手。根据用户的查询和上下文信息，分析用户意图并提供搜索建议。

请以JSON格式返回结果：
{
  "refined_intent": "精炼后的搜索意图描述",
  "suggested_terms": ["建议搜索词1", "建议搜索词2"],
  "confidence": 0.0-1.0之间的置信度
}

保持回复简洁，专注于帮助用户找到他们需要的文件或内容。"#;

/// Cloud inference bridge
pub struct CloudBridge {
    /// HTTP client
    client: Client,
    
    /// Cloud configuration
    config: CloudConfig,
    
    /// Rate limiter
    rate_limiter: RateLimiter,
    
    /// Cost tracker (database-backed)
    cost_tracker: Arc<CostTracker>,
    
    /// Data anonymizer
    anonymizer: DataAnonymizer,
}

/// Cloud configuration
#[derive(Debug, Clone)]
pub struct CloudConfig {
    /// API endpoint URL
    pub endpoint: String,
    
    /// API key (encrypted)
    pub api_key: SecretString,
    
    /// Model to use
    pub model: CloudModelType,
    
    /// Monthly cost limit in USD
    pub monthly_cost_limit: f64,
    
    /// Requests per minute limit
    pub requests_per_minute: u32,
    
    /// Whether cloud is enabled
    pub enabled: bool,
    
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    
    /// Maximum retries
    pub max_retries: u32,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: SecretString::new(String::new()),
            model: CloudModelType::GPT4oMini,
            monthly_cost_limit: 10.0,
            requests_per_minute: 60,
            enabled: false,
            timeout_ms: 5000,
            max_retries: 3,
        }
    }
}

impl CloudConfig {
    /// Create a new cloud config for OpenAI
    pub fn openai(api_key: String) -> Self {
        Self {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: SecretString::new(api_key),
            model: CloudModelType::GPT4oMini,
            enabled: true,
            ..Default::default()
        }
    }
    
    /// Create a new cloud config for Anthropic
    pub fn anthropic(api_key: String) -> Self {
        Self {
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            api_key: SecretString::new(api_key),
            model: CloudModelType::ClaudeHaiku,
            enabled: true,
            ..Default::default()
        }
    }
}

/// Cloud inference result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInferenceResult {
    /// Understanding from cloud
    pub understanding: CloudUnderstanding,
    
    /// Tokens used
    pub tokens_used: u32,
    
    /// Cost in USD
    pub cost_usd: f64,
    
    /// Inference duration in milliseconds
    pub duration_ms: u64,
}

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    /// Tokens available
    tokens: AtomicU64,
    
    /// Maximum tokens (requests per minute)
    max_tokens: u64,
    
    /// Last refill time
    last_refill: RwLock<Instant>,
    
    /// Refill interval
    refill_interval: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            tokens: AtomicU64::new(requests_per_minute as u64),
            max_tokens: requests_per_minute as u64,
            last_refill: RwLock::new(Instant::now()),
            refill_interval: Duration::from_secs(60),
        }
    }
    
    /// Try to acquire a token
    pub async fn acquire(&self) -> InferenceResult<()> {
        // Refill tokens if needed
        self.refill().await;
        
        // Try to acquire a token
        let current = self.tokens.load(Ordering::SeqCst);
        if current == 0 {
            return Err(InferenceError::RateLimitExceeded {
                retry_after_secs: 60,
            });
        }
        
        // Decrement token count
        self.tokens.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
    
    /// Refill tokens based on elapsed time
    async fn refill(&self) {
        let mut last_refill = self.last_refill.write().await;
        let elapsed = last_refill.elapsed();
        
        if elapsed >= self.refill_interval {
            self.tokens.store(self.max_tokens, Ordering::SeqCst);
            *last_refill = Instant::now();
        }
    }
    
    /// Get current available tokens
    pub fn available_tokens(&self) -> u64 {
        self.tokens.load(Ordering::SeqCst)
    }
}

/// Cost tracker for monitoring API usage - DATABASE BACKED
/// 
/// This implementation persists all usage records to SQLite, ensuring
/// that monthly cost limits work correctly across application restarts.
pub struct CostTracker {
    /// Database connection pool
    db: SqlitePool,
    
    /// Monthly limit in USD
    monthly_limit: f64,
    
    /// Cached current month cost (updated on each record)
    cached_cost: AtomicU64, // Stored as microdollars (1 USD = 1_000_000)
}

/// Usage record for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Tokens used
    pub tokens: u32,
    
    /// Cost in USD
    pub cost_usd: f64,
    
    /// Model used
    pub model: String,
}

impl CostTracker {
    /// Create a new cost tracker with database persistence
    pub async fn new(db: SqlitePool, monthly_limit: f64) -> InferenceResult<Self> {
        let tracker = Self {
            db,
            monthly_limit,
            cached_cost: AtomicU64::new(0),
        };
        
        // Load current month's cost from database
        tracker.reload_monthly_cost().await?;
        
        Ok(tracker)
    }
    
    /// Reload the current month's cost from database
    async fn reload_monthly_cost(&self) -> InferenceResult<()> {
        let now = Utc::now();
        let month_start = format!("{}-{:02}-01 00:00:00", now.year(), now.month());
        
        let result: Option<f64> = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(cost), 0.0)
            FROM cloud_usage
            WHERE timestamp >= ?
            "#
        )
        .bind(&month_start)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| InferenceError::DatabaseError {
            reason: format!("Failed to load monthly cost: {}", e),
        })?;
        
        let cost = result.unwrap_or(0.0);
        let cost_micros = (cost * 1_000_000.0) as u64;
        self.cached_cost.store(cost_micros, Ordering::SeqCst);
        
        tracing::info!("Loaded monthly cloud cost from database: ${:.4}", cost);
        
        Ok(())
    }
    
    /// Record usage to database
    pub async fn record(&self, tokens: u32, model: &CloudModelType) -> InferenceResult<()> {
        let cost = self.calculate_cost(tokens, model);
        let timestamp = Utc::now().to_rfc3339();
        let model_str = model.to_string();
        
        // Insert into database
        sqlx::query(
            r#"
            INSERT INTO cloud_usage (timestamp, tokens, cost, model, request_type)
            VALUES (?, ?, ?, ?, 'inference')
            "#
        )
        .bind(&timestamp)
        .bind(tokens as i64)
        .bind(cost)
        .bind(&model_str)
        .execute(&self.db)
        .await
        .map_err(|e| InferenceError::DatabaseError {
            reason: format!("Failed to record usage: {}", e),
        })?;
        
        // Update cached cost
        let cost_micros = (cost * 1_000_000.0) as u64;
        self.cached_cost.fetch_add(cost_micros, Ordering::SeqCst);
        
        tracing::debug!(
            "Recorded cloud usage: {} tokens, ${:.6}, model: {}",
            tokens, cost, model_str
        );
        
        Ok(())
    }
    
    /// Check if monthly limit is reached
    pub fn is_limit_reached(&self) -> bool {
        let current = self.cached_cost.load(Ordering::SeqCst) as f64 / 1_000_000.0;
        current >= self.monthly_limit
    }
    
    /// Get current cost in USD
    pub fn current_cost_usd(&self) -> f64 {
        self.cached_cost.load(Ordering::SeqCst) as f64 / 1_000_000.0
    }
    
    /// Get monthly limit in USD
    pub fn monthly_limit_usd(&self) -> f64 {
        self.monthly_limit
    }
    
    /// Calculate cost for tokens
    fn calculate_cost(&self, tokens: u32, model: &CloudModelType) -> f64 {
        // Pricing per 1M tokens (approximate, assuming mixed input/output)
        let price_per_million = match model {
            CloudModelType::GPT4oMini => 0.375, // Average of $0.15 input, $0.60 output
            CloudModelType::ClaudeHaiku => 0.625, // Average of $0.25 input, $1.00 output
            CloudModelType::Custom(_) => 0.5, // Default estimate
        };
        
        (tokens as f64) * price_per_million / 1_000_000.0
    }
    
    /// Reset cache for new month (called on month boundary)
    pub async fn check_month_reset(&self) -> InferenceResult<()> {
        // Reload from database - this will automatically get only current month's data
        self.reload_monthly_cost().await
    }
    
    /// Get usage statistics from database
    pub async fn get_stats(&self) -> InferenceResult<CostStats> {
        let now = Utc::now();
        let month_start = format!("{}-{:02}-01 00:00:00", now.year(), now.month());
        
        let row: (i64, i64) = sqlx::query_as(
            r#"
            SELECT 
                COUNT(*) as total_requests,
                COALESCE(SUM(tokens), 0) as total_tokens
            FROM cloud_usage
            WHERE timestamp >= ?
            "#
        )
        .bind(&month_start)
        .fetch_one(&self.db)
        .await
        .map_err(|e| InferenceError::DatabaseError {
            reason: format!("Failed to get stats: {}", e),
        })?;
        
        Ok(CostStats {
            current_cost_usd: self.current_cost_usd(),
            monthly_limit_usd: self.monthly_limit_usd(),
            total_requests: row.0 as usize,
            total_tokens: row.1 as u64,
        })
    }
    
    /// Get recent usage records
    pub async fn get_recent_records(&self, limit: u32) -> InferenceResult<Vec<UsageRecord>> {
        let rows: Vec<(String, i64, f64, Option<String>)> = sqlx::query_as(
            r#"
            SELECT timestamp, tokens, cost, model
            FROM cloud_usage
            ORDER BY timestamp DESC
            LIMIT ?
            "#
        )
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await
        .map_err(|e| InferenceError::DatabaseError {
            reason: format!("Failed to get records: {}", e),
        })?;
        
        let records = rows.into_iter().map(|(ts, tokens, cost, model)| {
            UsageRecord {
                timestamp: DateTime::parse_from_rfc3339(&ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                tokens: tokens as u32,
                cost_usd: cost,
                model: model.unwrap_or_else(|| "unknown".to_string()),
            }
        }).collect();
        
        Ok(records)
    }
}

/// Cost statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostStats {
    /// Current month's cost in USD
    pub current_cost_usd: f64,
    
    /// Monthly limit in USD
    pub monthly_limit_usd: f64,
    
    /// Total requests this month
    pub total_requests: usize,
    
    /// Total tokens used this month
    pub total_tokens: u64,
}

/// Cloud API request structure
#[derive(Debug, Serialize)]
struct CloudApiRequest {
    model: String,
    messages: Vec<CloudMessage>,
    max_tokens: u32,
    temperature: f32,
}

/// Cloud API message
#[derive(Debug, Serialize)]
struct CloudMessage {
    role: String,
    content: String,
}

/// Cloud API response structure
#[derive(Debug, Deserialize)]
struct CloudApiResponse {
    choices: Vec<CloudChoice>,
    usage: CloudUsage,
}

#[derive(Debug, Deserialize)]
struct CloudChoice {
    message: CloudResponseMessage,
}

#[derive(Debug, Deserialize)]
struct CloudResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct CloudUsage {
    total_tokens: u32,
}

/// Parsed cloud response
#[derive(Debug, Deserialize)]
struct ParsedCloudResponse {
    refined_intent: Option<String>,
    suggested_terms: Option<Vec<String>>,
    confidence: Option<f32>,
}

impl CloudBridge {
    /// Create a new cloud bridge with database-backed cost tracking
    pub async fn new(config: CloudConfig, db: SqlitePool) -> InferenceResult<Self> {
        let rate_limiter = RateLimiter::new(config.requests_per_minute);
        let cost_tracker = Arc::new(
            CostTracker::new(db, config.monthly_cost_limit).await?
        );
        
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| InferenceError::NetworkError {
                reason: format!("Failed to create HTTP client: {}", e),
            })?;
        
        Ok(Self {
            client,
            config,
            rate_limiter,
            cost_tracker,
            anonymizer: DataAnonymizer::default(),
        })
    }
    
    /// Check if cloud is available
    pub fn is_available(&self) -> bool {
        self.config.enabled && !self.cost_tracker.is_limit_reached()
    }
    
    /// Perform cloud inference
    pub async fn infer(&self, request: &InferenceRequest, prompt: &str) -> InferenceResult<CloudInferenceResult> {
        // Check if enabled
        if !self.config.enabled {
            return Err(InferenceError::CloudUnavailable {
                reason: "Cloud inference is disabled".to_string(),
            });
        }
        
        // Check cost limit
        if self.cost_tracker.is_limit_reached() {
            return Err(InferenceError::CostLimitReached {
                current: self.cost_tracker.current_cost_usd(),
                limit: self.cost_tracker.monthly_limit_usd(),
            });
        }
        
        // Acquire rate limit token
        self.rate_limiter.acquire().await?;
        
        // Check for month reset
        self.cost_tracker.check_month_reset().await?;
        
        // Anonymize the prompt
        let anonymized_prompt = self.anonymizer.anonymize(prompt);
        
        let start = Instant::now();
        
        // Make API request with retries
        let response = self.make_request_with_retry(&anonymized_prompt).await?;
        
        let duration_ms = start.elapsed().as_millis() as u64;
        
        // Record usage to database
        self.cost_tracker.record(response.usage.total_tokens, &self.config.model).await?;
        
        // Parse response
        let understanding = self.parse_response(&response)?;
        let cost_usd = self.cost_tracker.current_cost_usd();
        
        Ok(CloudInferenceResult {
            understanding,
            tokens_used: response.usage.total_tokens,
            cost_usd,
            duration_ms,
        })
    }
    
    /// Make API request with retry logic
    async fn make_request_with_retry(&self, prompt: &str) -> InferenceResult<CloudApiResponse> {
        let mut last_error = None;
        let mut retry_delay = Duration::from_millis(100);
        
        for attempt in 0..self.config.max_retries {
            match self.make_request(prompt).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        "Cloud API request failed (attempt {}/{}): {}",
                        attempt + 1,
                        self.config.max_retries,
                        e
                    );
                    
                    // Check if error is retryable
                    if !self.is_retryable_error(&e) {
                        return Err(e);
                    }
                    
                    // Handle rate limit with Retry-After header
                    if let InferenceError::RateLimitExceeded { retry_after_secs } = &e {
                        retry_delay = Duration::from_secs(*retry_after_secs);
                    }
                    
                    last_error = Some(e);
                    
                    // Wait before retry (exponential backoff)
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = retry_delay.saturating_mul(2);
                }
            }
        }
        
        Err(last_error.unwrap_or(InferenceError::CloudApiError {
            reason: "Max retries exceeded".to_string(),
        }))
    }
    
    /// Make a single API request
    async fn make_request(&self, prompt: &str) -> InferenceResult<CloudApiResponse> {
        let request_body = CloudApiRequest {
            model: self.config.model.to_string(),
            messages: vec![
                CloudMessage {
                    role: "system".to_string(),
                    content: SYSTEM_PROMPT.to_string(),
                },
                CloudMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                },
            ],
            max_tokens: 500,
            temperature: 0.3,
        };
        
        let response = self.client
            .post(&self.config.endpoint)
            .header("Authorization", format!("Bearer {}", self.config.api_key.expose_secret()))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| InferenceError::NetworkError {
                reason: format!("Request failed: {}", e),
            })?;
        
        // Check for rate limit response - read Retry-After header
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            
            return Err(InferenceError::RateLimitExceeded {
                retry_after_secs: retry_after,
            });
        }
        
        // Check for other errors
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(InferenceError::CloudApiError {
                reason: format!("HTTP {}: {}", status, body),
            });
        }
        
        let api_response: CloudApiResponse = response.json().await
            .map_err(|e| InferenceError::CloudApiError {
                reason: format!("Failed to parse response: {}", e),
            })?;
        
        Ok(api_response)
    }
    
    /// Parse cloud API response into understanding
    fn parse_response(&self, response: &CloudApiResponse) -> InferenceResult<CloudUnderstanding> {
        let content = response.choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("");
        
        // Try to parse as JSON
        let parsed: ParsedCloudResponse = serde_json::from_str(content)
            .unwrap_or(ParsedCloudResponse {
                refined_intent: Some(content.to_string()),
                suggested_terms: None,
                confidence: Some(0.5),
            });
        
        Ok(CloudUnderstanding {
            refined_intent: parsed.refined_intent,
            suggested_terms: parsed.suggested_terms.unwrap_or_default(),
            confidence: parsed.confidence.unwrap_or(0.5),
            raw_response: Some(content.to_string()),
        })
    }
    
    /// Check if an error is retryable
    fn is_retryable_error(&self, error: &InferenceError) -> bool {
        matches!(
            error,
            InferenceError::NetworkError { .. }
            | InferenceError::Timeout { .. }
            | InferenceError::RateLimitExceeded { .. }
        )
    }
    
    /// Get cost tracker reference
    pub fn cost_tracker(&self) -> &Arc<CostTracker> {
        &self.cost_tracker
    }
    
    /// Get rate limiter reference
    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }
    
    /// Get current configuration
    pub fn config(&self) -> &CloudConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(60);
        assert_eq!(limiter.available_tokens(), 60);
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire() {
        let limiter = RateLimiter::new(2);
        
        // Should succeed twice
        assert!(limiter.acquire().await.is_ok());
        assert!(limiter.acquire().await.is_ok());
        
        // Should fail on third
        assert!(limiter.acquire().await.is_err());
    }

    #[test]
    fn test_cloud_config_default() {
        let config = CloudConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.monthly_cost_limit, 10.0);
        assert_eq!(config.requests_per_minute, 60);
    }

    #[test]
    fn test_cloud_config_openai() {
        let config = CloudConfig::openai("test-key".to_string());
        assert!(config.enabled);
        assert!(config.endpoint.contains("openai"));
        assert!(matches!(config.model, CloudModelType::GPT4oMini));
    }

    #[test]
    fn test_cloud_config_anthropic() {
        let config = CloudConfig::anthropic("test-key".to_string());
        assert!(config.enabled);
        assert!(config.endpoint.contains("anthropic"));
        assert!(matches!(config.model, CloudModelType::ClaudeHaiku));
    }
}
