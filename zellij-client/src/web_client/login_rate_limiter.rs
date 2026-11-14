use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, Ipv6Addr};
use std::time::{Duration, Instant};

/// How many login attempts a single address may make in a burst.
pub const LOGIN_BURST: u32 = 10;
/// How long it takes a single address to earn back one login attempt.
pub const LOGIN_REFILL_INTERVAL: Duration = Duration::from_secs(3);
/// Upper bound on the number of addresses that are tracked at the same time.
pub const MAX_TRACKED_ADDRESSES: usize = 4096;

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last_seen: Instant,
}

/// A per-address token bucket for the unauthenticated login endpoint.
///
/// Every login attempt (successful or not) costs one token. The memory used is
/// bounded: once `MAX_TRACKED_ADDRESSES` addresses are tracked, the one that
/// has been silent the longest is forgotten to make room.
#[derive(Debug, Default)]
pub struct LoginRateLimiter {
    buckets: HashMap<IpAddr, Bucket>,
    by_last_seen: BTreeSet<(Instant, IpAddr)>,
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `Ok(())` if the attempt is allowed, or `Err(retry_after)` with the
    /// time after which the address can try again.
    pub fn check(&mut self, address: IpAddr) -> Result<(), Duration> {
        self.check_at(address, Instant::now())
    }

    fn check_at(&mut self, address: IpAddr, now: Instant) -> Result<(), Duration> {
        let address = normalize(address);
        if !self.buckets.contains_key(&address) && self.buckets.len() >= MAX_TRACKED_ADDRESSES {
            self.make_room(now);
        }
        if let Some(last_seen) = self.buckets.get(&address).map(|bucket| bucket.last_seen) {
            self.by_last_seen.remove(&(last_seen, address));
        }
        let bucket = self.buckets.entry(address).or_insert(Bucket {
            tokens: LOGIN_BURST as f64,
            last_seen: now,
        });
        let earned = now
            .saturating_duration_since(bucket.last_seen)
            .as_secs_f64()
            / LOGIN_REFILL_INTERVAL.as_secs_f64();
        bucket.tokens = (bucket.tokens + earned).min(LOGIN_BURST as f64);
        bucket.last_seen = now;
        let verdict = if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let missing = 1.0 - bucket.tokens;
            Err(Duration::from_secs_f64(
                missing * LOGIN_REFILL_INTERVAL.as_secs_f64(),
            ))
        };
        self.by_last_seen.insert((now, address));
        verdict
    }

    fn make_room(&mut self, now: Instant) {
        // addresses that have fully recovered carry no information
        let full_after = LOGIN_REFILL_INTERVAL * LOGIN_BURST;
        while let Some(&(last_seen, address)) = self.by_last_seen.first() {
            if now.saturating_duration_since(last_seen) < full_after {
                break;
            }
            self.by_last_seen.pop_first();
            self.buckets.remove(&address);
        }
        if self.buckets.len() >= MAX_TRACKED_ADDRESSES {
            if let Some((_, oldest)) = self.by_last_seen.pop_first() {
                self.buckets.remove(&oldest);
            }
        }
    }
}

/// A whole IPv6 /64 is routinely handed to a single subscriber, so limiting
/// individual IPv6 addresses would be trivial to get around.
fn normalize(address: IpAddr) -> IpAddr {
    match address.to_canonical() {
        IpAddr::V6(v6) => {
            let [a, b, c, d, ..] = v6.segments();
            IpAddr::V6(Ipv6Addr::new(a, b, c, d, 0, 0, 0, 0))
        },
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    #[test]
    fn allows_a_burst_then_refuses() {
        let mut limiter = LoginRateLimiter::new();
        let now = Instant::now();
        for _ in 0..LOGIN_BURST {
            assert!(limiter.check_at(ip(1), now).is_ok());
        }
        let retry_after = limiter.check_at(ip(1), now).unwrap_err();
        assert!(retry_after > Duration::ZERO);
        assert!(retry_after <= LOGIN_REFILL_INTERVAL);
    }

    #[test]
    fn addresses_are_limited_independently() {
        let mut limiter = LoginRateLimiter::new();
        let now = Instant::now();
        for _ in 0..LOGIN_BURST {
            limiter.check_at(ip(1), now).unwrap();
        }
        assert!(limiter.check_at(ip(1), now).is_err());
        assert!(limiter.check_at(ip(2), now).is_ok());
    }

    #[test]
    fn ipv6_addresses_in_one_subnet_share_a_bucket() {
        let mut limiter = LoginRateLimiter::new();
        let now = Instant::now();
        for n in 0..LOGIN_BURST {
            let address = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 2, 0, 0, 0, n as u16));
            limiter.check_at(address, now).unwrap();
        }
        let neighbour = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 2, 9, 9, 9, 9));
        assert!(limiter.check_at(neighbour, now).is_err());
        let other_subnet = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 3, 0, 0, 0, 1));
        assert!(limiter.check_at(other_subnet, now).is_ok());
    }

    #[test]
    fn tokens_come_back_over_time() {
        let mut limiter = LoginRateLimiter::new();
        let now = Instant::now();
        for _ in 0..LOGIN_BURST {
            limiter.check_at(ip(1), now).unwrap();
        }
        assert!(limiter.check_at(ip(1), now).is_err());
        let later = now + LOGIN_REFILL_INTERVAL;
        assert!(limiter.check_at(ip(1), later).is_ok());
        assert!(limiter.check_at(ip(1), later).is_err());
    }

    #[test]
    fn tracked_addresses_are_bounded() {
        let mut limiter = LoginRateLimiter::new();
        let now = Instant::now();
        for n in 0..(MAX_TRACKED_ADDRESSES as u32 + 100) {
            let address = IpAddr::V4(Ipv4Addr::from(n));
            let _ = limiter.check_at(address, now + Duration::from_millis(n as u64));
        }
        assert!(limiter.buckets.len() <= MAX_TRACKED_ADDRESSES);
        assert_eq!(limiter.by_last_seen.len(), limiter.buckets.len());
        assert!(!limiter.buckets.contains_key(&IpAddr::V4(Ipv4Addr::from(0))));
    }
}
