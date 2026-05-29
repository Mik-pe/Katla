use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) enum RateLimitResult {
    Allowed,
    Wait(Duration),
    Exceeded { retry_after: Duration },
}

pub(crate) struct RateLimiter {
    min_interval: Duration,
    max_calls_per_minute: u32,
    state: Mutex<RateLimiterState>,
}

struct RateLimiterState {
    last_call: Option<Instant>,
    call_timestamps: Vec<Instant>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration, max_calls_per_minute: u32) -> Self {
        Self {
            min_interval,
            max_calls_per_minute: max_calls_per_minute.max(1),
            state: Mutex::new(RateLimiterState {
                last_call: None,
                call_timestamps: Vec::new(),
            }),
        }
    }

    pub fn check_and_record(&self) -> RateLimitResult {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let window = Duration::from_secs(60);

        state
            .call_timestamps
            .retain(|&t| now.duration_since(t) < window);

        if state.call_timestamps.len() >= self.max_calls_per_minute as usize {
            let oldest = state.call_timestamps[0];
            let retry_after = oldest + window - now;
            return RateLimitResult::Exceeded { retry_after };
        }

        if let Some(last) = state.last_call {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_interval {
                return RateLimitResult::Wait(self.min_interval - elapsed);
            }
        }

        state.last_call = Some(now);
        state.call_timestamps.push(now);
        RateLimitResult::Allowed
    }

    pub fn record(&self) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.last_call = Some(now);
        state.call_timestamps.push(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_call_allowed() {
        let limiter = RateLimiter::new(Duration::from_millis(100), 10);
        assert!(matches!(
            limiter.check_and_record(),
            RateLimitResult::Allowed
        ));
    }

    #[test]
    fn test_rapid_call_waits() {
        let limiter = RateLimiter::new(Duration::from_millis(100), 10);
        limiter.check_and_record();
        match limiter.check_and_record() {
            RateLimitResult::Wait(d) => assert!(d <= Duration::from_millis(100)),
            other => panic!("Expected Wait, got {:?}", other),
        }
    }

    #[test]
    fn test_max_calls_exceeded() {
        let limiter = RateLimiter::new(Duration::from_millis(0), 3);
        limiter.check_and_record();
        limiter.check_and_record();
        limiter.check_and_record();
        match limiter.check_and_record() {
            RateLimitResult::Exceeded { .. } => {}
            other => panic!("Expected Exceeded, got {:?}", other),
        }
    }

    #[test]
    fn test_interval_reset_after_wait() {
        let limiter = RateLimiter::new(Duration::from_millis(10), 10);
        limiter.check_and_record();
        std::thread::sleep(Duration::from_millis(15));
        assert!(matches!(
            limiter.check_and_record(),
            RateLimitResult::Allowed
        ));
    }

    #[test]
    fn test_record_increments_count() {
        let limiter = RateLimiter::new(Duration::from_millis(0), 2);
        limiter.check_and_record();
        limiter.check_and_record();
        assert!(matches!(
            limiter.check_and_record(),
            RateLimitResult::Exceeded { .. }
        ));
    }

    #[test]
    fn test_max_calls_at_least_one() {
        let limiter = RateLimiter::new(Duration::from_millis(0), 0);
        assert!(matches!(
            limiter.check_and_record(),
            RateLimitResult::Allowed
        ));
    }
}
