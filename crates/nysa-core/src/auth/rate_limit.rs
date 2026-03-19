use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

const DEFAULT_USER_WINDOW_MINUTES: i64 = 15;
const DEFAULT_USER_MAX_ATTEMPTS: u32 = 5;
const DEFAULT_IP_WINDOW_MINUTES: i64 = 15;
const DEFAULT_IP_MAX_ATTEMPTS: u32 = 10;
const CLEANUP_INTERVAL_MINUTES: u64 = 5;

#[derive(Debug, Clone)]
struct RateLimitEntry {
    attempts: Vec<DateTime<Utc>>,
    window_start: DateTime<Utc>,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            attempts: vec![],
            window_start: Utc::now(),
        }
    }

    fn is_expired(&self, window_duration: Duration) -> bool {
        let window_end = self.window_start
            + chrono::Duration::from_std(window_duration).unwrap_or(chrono::Duration::minutes(15));
        Utc::now() > window_end
    }

    fn add_attempt(&mut self) {
        self.attempts.push(Utc::now());
    }

    fn count_in_window(&self, window_duration: Duration) -> usize {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(window_duration).unwrap_or(chrono::Duration::minutes(15));
        self.attempts.iter().filter(|&&t| t > cutoff).count()
    }

    fn get_oldest_attempt_in_window(&self, window_duration: Duration) -> Option<DateTime<Utc>> {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(window_duration).unwrap_or(chrono::Duration::minutes(15));
        self.attempts.iter().filter(|&&t| t > cutoff).min().copied()
    }

    fn reset(&mut self) {
        self.attempts.clear();
        self.window_start = Utc::now();
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub retry_after: Option<Duration>,
}

pub struct RateLimiter {
    user_attempts: Arc<DashMap<String, RateLimitEntry>>,
    ip_attempts: Arc<DashMap<String, RateLimitEntry>>,
    user_max_attempts: u32,
    user_window: Duration,
    ip_max_attempts: u32,
    ip_window: Duration,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            user_attempts: Arc::new(DashMap::new()),
            ip_attempts: Arc::new(DashMap::new()),
            user_max_attempts: DEFAULT_USER_MAX_ATTEMPTS,
            user_window: Duration::from_secs(DEFAULT_USER_WINDOW_MINUTES as u64 * 60),
            ip_max_attempts: DEFAULT_IP_MAX_ATTEMPTS,
            ip_window: Duration::from_secs(DEFAULT_IP_WINDOW_MINUTES as u64 * 60),
        }
    }

    pub fn with_limits(
        user_max: u32,
        user_window_minutes: i64,
        ip_max: u32,
        ip_window_minutes: i64,
    ) -> Self {
        Self {
            user_attempts: Arc::new(DashMap::new()),
            ip_attempts: Arc::new(DashMap::new()),
            user_max_attempts: user_max,
            user_window: Duration::from_secs(user_window_minutes as u64 * 60),
            ip_max_attempts: ip_max,
            ip_window: Duration::from_secs(ip_window_minutes as u64 * 60),
        }
    }

    /// Check if a user is rate limited
    pub fn check_user(&self, user_id: &str) -> RateLimitResult {
        self.check_limit(
            user_id,
            &self.user_attempts,
            self.user_max_attempts,
            self.user_window,
        )
    }

    /// Record a user attempt
    pub fn record_user_attempt(&self, user_id: &str) {
        self.record_attempt(user_id, &self.user_attempts, self.user_window);
    }

    /// Check if an IP is rate limited
    pub fn check_ip(&self, ip: &str) -> RateLimitResult {
        self.check_limit(ip, &self.ip_attempts, self.ip_max_attempts, self.ip_window)
    }

    /// Record an IP attempt
    pub fn record_ip_attempt(&self, ip: &str) {
        self.record_attempt(ip, &self.ip_attempts, self.ip_window);
    }

    /// Check both user and IP limits
    pub fn check_both(&self, user_id: &str, ip: Option<&str>) -> RateLimitResult {
        let user_result = self.check_user(user_id);

        if !user_result.allowed {
            return user_result;
        }

        if let Some(ip) = ip {
            let ip_result = self.check_ip(ip);
            if !ip_result.allowed {
                return ip_result;
            }
        }

        user_result
    }

    /// Record both user and IP attempts
    pub fn record_attempts(&self, user_id: &str, ip: Option<&str>) {
        self.record_user_attempt(user_id);
        if let Some(ip) = ip {
            self.record_ip_attempt(ip);
        }
    }

    fn check_limit(
        &self,
        key: &str,
        map: &DashMap<String, RateLimitEntry>,
        max_attempts: u32,
        window: Duration,
    ) -> RateLimitResult {
        let entry = match map.get(key) {
            Some(e) => e,
            None => {
                return RateLimitResult {
                    allowed: true,
                    remaining: max_attempts,
                    retry_after: None,
                };
            }
        };

        // Check if the window has expired
        if entry.is_expired(window) {
            return RateLimitResult {
                allowed: true,
                remaining: max_attempts,
                retry_after: None,
            };
        }

        let attempts_in_window = entry.count_in_window(window) as u32;

        if attempts_in_window >= max_attempts {
            // Calculate retry_after
            let oldest_attempt = entry.get_oldest_attempt_in_window(window);
            let retry_after = oldest_attempt.map(|oldest| {
                let window_end = oldest
                    + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::minutes(15));
                let now = Utc::now();
                if window_end > now {
                    let diff = window_end - now;
                    Duration::from_secs(diff.num_seconds() as u64)
                } else {
                    Duration::from_secs(0)
                }
            });

            RateLimitResult {
                allowed: false,
                remaining: 0,
                retry_after,
            }
        } else {
            RateLimitResult {
                allowed: true,
                remaining: max_attempts - attempts_in_window,
                retry_after: None,
            }
        }
    }

    fn record_attempt(&self, key: &str, map: &DashMap<String, RateLimitEntry>, window: Duration) {
        map.entry(key.to_string())
            .and_modify(|entry| {
                if entry.is_expired(window) {
                    entry.reset();
                }
                entry.add_attempt();
            })
            .or_insert_with(|| {
                let mut entry = RateLimitEntry::new();
                entry.add_attempt();
                entry
            });
    }

    /// Start a background cleanup task
    pub fn start_cleanup_task(&self) {
        let user_attempts = Arc::clone(&self.user_attempts);
        let ip_attempts = Arc::clone(&self.ip_attempts);
        let user_window = self.user_window;
        let ip_window = self.ip_window;

        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(CLEANUP_INTERVAL_MINUTES * 60));

            loop {
                cleanup_interval.tick().await;

                // Clean up expired user entries
                let expired_users: Vec<String> = user_attempts
                    .iter()
                    .filter(|entry| entry.is_expired(user_window))
                    .map(|entry| entry.key().clone())
                    .collect();

                for key in expired_users {
                    user_attempts.remove(&key);
                }

                // Clean up expired IP entries
                let expired_ips: Vec<String> = ip_attempts
                    .iter()
                    .filter(|entry| entry.is_expired(ip_window))
                    .map(|entry| entry.key().clone())
                    .collect();

                for key in expired_ips {
                    ip_attempts.remove(&key);
                }

                tracing::debug!(
                    "Rate limiter cleanup: {} user entries, {} IP entries remaining",
                    user_attempts.len(),
                    ip_attempts.len()
                );
            }
        });
    }

    /// Get current stats for debugging
    pub fn stats(&self) -> (usize, usize) {
        (self.user_attempts.len(), self.ip_attempts.len())
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiting() {
        let limiter = RateLimiter::with_limits(2, 1, 5, 1);
        let user_id = "test_user";

        // First attempt should be allowed
        let result = limiter.check_user(user_id);
        assert!(result.allowed);
        assert_eq!(result.remaining, 2);

        // Record first attempt
        limiter.record_user_attempt(user_id);
        let result = limiter.check_user(user_id);
        assert!(result.allowed);
        assert_eq!(result.remaining, 1);

        // Record second attempt
        limiter.record_user_attempt(user_id);
        let result = limiter.check_user(user_id);
        assert!(!result.allowed); // Now rate limited
        assert_eq!(result.remaining, 0);
        assert!(result.retry_after.is_some());
    }
}
