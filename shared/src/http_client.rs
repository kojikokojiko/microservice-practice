//! HTTP client with timeout, retry (exponential backoff), and circuit breaker for outbound calls.

use reqwest::Client;
use std::error::Error;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use std::time::Instant;
use tokio::time::sleep;

/// Error type for outbound HTTP calls (reqwest or circuit/open/retry).
pub type HttpClientError = Box<dyn Error + Send + Sync>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_COUNT: u32 = 3;
const CIRCUIT_FAILURE_THRESHOLD: u32 = 5;
const CIRCUIT_OPEN_DURATION: Duration = Duration::from_secs(30);

/// Shared HTTP client with timeout. Retry and circuit breaker are applied per-call in ServiceClient.
pub fn default_client() -> Client {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("reqwest client")
}

/// Circuit breaker state for one target (e.g. admin-service).
#[derive(Debug)]
struct CircuitState {
    failures: AtomicU32,
    last_failure: std::sync::Mutex<Option<Instant>>,
}

impl CircuitState {
    fn new() -> Self {
        Self {
            failures: AtomicU32::new(0),
            last_failure: std::sync::Mutex::new(None),
        }
    }

    fn record_success(&self) {
        self.failures.store(0, Ordering::SeqCst);
        *self.last_failure.lock().unwrap() = None;
    }

    fn record_failure(&self) {
        let n = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure.lock().unwrap() = Some(Instant::now());
        if n >= CIRCUIT_FAILURE_THRESHOLD {
            tracing::warn!("circuit open after {} failures", n);
        }
    }

    fn is_open(&self) -> bool {
        let failures = self.failures.load(Ordering::SeqCst);
        if failures < CIRCUIT_FAILURE_THRESHOLD {
            return false;
        }
        let last = self.last_failure.lock().unwrap();
        match *last {
            Some(t) if t.elapsed() >= CIRCUIT_OPEN_DURATION => {
                // half-open: allow one request
                false
            }
            Some(_) => true,
            None => true,
        }
    }
}

/// Client for calling other services with retry and circuit breaker.
pub struct ServiceClient {
    client: Client,
    admin_base: String,
    teacher_base: String,
    admin_circuit: std::sync::Arc<CircuitState>,
    teacher_circuit: std::sync::Arc<CircuitState>,
}

impl ServiceClient {
    /// base_url e.g. http://admin-service:8080 (without trailing slash)
    pub fn new(admin_base: String, teacher_base: String) -> Self {
        Self {
            client: default_client(),
            admin_base,
            teacher_base,
            admin_circuit: std::sync::Arc::new(CircuitState::new()),
            teacher_circuit: std::sync::Arc::new(CircuitState::new()),
        }
    }

    /// GET admin-service e.g. /api/admin/courses/{id}
    /// bearer_token: optional "Bearer <jwt>" for forwarding auth to admin-service
    pub async fn get_admin(
        &self,
        path: &str,
        bearer_token: Option<&str>,
    ) -> Result<reqwest::Response, HttpClientError> {
        if self.admin_circuit.is_open() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "circuit open (admin-service)",
            )));
        }
        let url = format!("{}{}", self.admin_base, path);
        let res = self.request_with_retry(&url, bearer_token).await;
        if res.is_ok() {
            self.admin_circuit.record_success();
        } else {
            self.admin_circuit.record_failure();
        }
        res
    }

    /// GET teacher-service e.g. /api/teacher/assignments/{id}
    /// bearer_token: optional "Bearer <jwt>" for forwarding auth
    pub async fn get_teacher(
        &self,
        path: &str,
        bearer_token: Option<&str>,
    ) -> Result<reqwest::Response, HttpClientError> {
        if self.teacher_circuit.is_open() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "circuit open (teacher-service)",
            )));
        }
        let url = format!("{}{}", self.teacher_base, path);
        let res = self.request_with_retry(&url, bearer_token).await;
        if res.is_ok() {
            self.teacher_circuit.record_success();
        } else {
            self.teacher_circuit.record_failure();
        }
        res
    }

    async fn request_with_retry(
        &self,
        url: &str,
        bearer_token: Option<&str>,
    ) -> Result<reqwest::Response, HttpClientError> {
        let mut last_err: Option<HttpClientError> = None;
        for attempt in 0..=RETRY_COUNT {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                sleep(backoff).await;
            }
            let mut req = self.client.get(url);
            if let Some(t) = bearer_token {
                req = req.header("Authorization", t);
            }
            match req.send().await {
                Ok(res) => {
                    if res.status().is_success() {
                        return Ok(res);
                    }
                    last_err = Some(Box::new(res.error_for_status().unwrap_err()));
                }
                Err(e) => last_err = Some(Box::new(e)),
            }
        }
        Err(last_err.unwrap_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "no response after retries",
            ))
        }))
    }
}