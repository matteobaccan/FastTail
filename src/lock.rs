// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The PIN lock shared by the GUI and the terminal interface: the stored PIN and its
//! check, the wrong-attempt counter and its cooldown, and the idle clock that arms the
//! terminal's idle lock. Both interfaces use this code, so a PIN set in one opens the
//! other with the same rules.

use crate::config::FastTailConfig;
use std::time::{Duration, Instant};

/// Unlock phrase that always works, whatever the PIN is — a nod to WarGames.
pub const LOCK_BACKDOOR: &str = "joshua";

/// Scrambles a PIN before it is written to the ini file. FNV-1a over a fixed salt plus
/// the digits: it keeps the PIN from being read at a glance out of `fasttail.ini`, and
/// that is the whole of its ambition. The lock is a deterrent against someone walking
/// past the screen, not a security boundary — the log files stay readable on disk and
/// `LOCK_BACKDOOR` opens it anyway.
pub fn scramble_pin(pin: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in b"fasttail-lock-v1".iter().chain(pin.as_bytes()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Whether `attempt` opens a lock whose scrambled PIN is `stored`.
pub fn pin_matches(stored: &str, attempt: &str) -> bool {
    let attempt = attempt.trim();
    if attempt.eq_ignore_ascii_case(LOCK_BACKDOOR) {
        return true;
    }
    !stored.is_empty() && scramble_pin(attempt) == stored
}

/// A PIN the lock will accept: 4 to 12 digits.
pub fn is_valid_pin(pin: &str) -> bool {
    let pin = pin.trim();
    (4..=12).contains(&pin.chars().count()) && pin.chars().all(|c| c.is_ascii_digit())
}

/// Wrong PIN attempts on the lock screen. Three in a row close the prompt for a minute,
/// so guessing a 4-digit PIN costs hours instead of seconds; a correct PIN clears the
/// count. The state is deliberately in memory only: it is a deterrent, and a restart
/// clearing it changes nothing an attacker could not do by editing `fasttail.ini`.
#[derive(Debug, Default, Clone)]
pub struct LockAttempts {
    failures: u32,
    retry_at: Option<Instant>,
}

/// Wrong attempts allowed before the prompt pauses.
pub const LOCK_MAX_FAILURES: u32 = 3;
/// How long the prompt stays closed after those attempts.
pub const LOCK_COOLDOWN: Duration = Duration::from_secs(60);

impl LockAttempts {
    /// Registers a wrong PIN and returns whether it started a cooldown.
    pub fn register_failure(&mut self, now: Instant) -> bool {
        self.failures += 1;
        if self.failures.is_multiple_of(LOCK_MAX_FAILURES) {
            self.retry_at = Some(now + LOCK_COOLDOWN);
            true
        } else {
            false
        }
    }

    /// Time left before the next attempt is accepted, `None` when it is accepted now.
    pub fn cooldown_left(&self, now: Instant) -> Option<Duration> {
        let retry_at = self.retry_at?;
        (retry_at > now).then(|| retry_at - now)
    }

    /// Clears everything after a correct PIN.
    pub fn reset(&mut self) {
        self.failures = 0;
        self.retry_at = None;
    }
}

/// Whether `config` lets the window lock at all: a PIN is set. Without one no action may
/// lock, so the user can never lock themselves out.
pub fn can_lock(config: &FastTailConfig) -> bool {
    !config.lock_pin.is_empty()
}

/// Whether the idle lock is armed: a PIN, the master switch (`lock_enabled`) and an idle
/// time (`screensaver_timeout_mins` above zero). The GUI arms it through its screensaver;
/// the terminal, which has none, locks straight away when the time is up.
pub fn idle_lock_armed(config: &FastTailConfig) -> bool {
    can_lock(config) && config.lock_enabled && config.screensaver_timeout_mins > 0
}

/// Time since the last key or mouse event.
#[derive(Debug, Clone, Copy)]
pub struct IdleClock {
    last_input: Instant,
}

impl IdleClock {
    pub fn new(now: Instant) -> Self {
        Self { last_input: now }
    }

    /// A key or mouse event happened at `now`.
    pub fn touch(&mut self, now: Instant) {
        self.last_input = now;
    }

    pub fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_input)
    }

    /// Whether the idle lock should close the window at `now` (see `idle_lock_armed`).
    pub fn idle_lock_due(&self, now: Instant, config: &FastTailConfig) -> bool {
        idle_lock_armed(config)
            && self.idle_for(now)
                >= Duration::from_secs(u64::from(config.screensaver_timeout_mins) * 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_wrong_pins_pause_the_prompt_for_a_minute() {
        use std::time::{Duration, Instant};

        let now = Instant::now();
        let mut attempts = LockAttempts::default();
        assert!(!attempts.register_failure(now));
        assert!(!attempts.register_failure(now));
        assert!(
            attempts.cooldown_left(now).is_none(),
            "two misses cost nothing"
        );

        assert!(
            attempts.register_failure(now),
            "the third one starts the pause"
        );
        let left = attempts.cooldown_left(now).expect("prompt is paused");
        assert!(left <= LOCK_COOLDOWN && left > LOCK_COOLDOWN - Duration::from_secs(1));
        assert!(attempts
            .cooldown_left(now + LOCK_COOLDOWN - Duration::from_secs(1))
            .is_some());
        assert!(attempts.cooldown_left(now + LOCK_COOLDOWN).is_none());

        // Three more misses pause it again, and a correct PIN forgets everything.
        for _ in 0..3 {
            attempts.register_failure(now + LOCK_COOLDOWN);
        }
        assert!(attempts.cooldown_left(now + LOCK_COOLDOWN).is_some());
        attempts.reset();
        assert!(attempts.cooldown_left(now + LOCK_COOLDOWN).is_none());
    }

    #[test]
    fn the_idle_lock_needs_a_pin_the_switch_and_a_time() {
        let now = Instant::now();
        let mut clock = IdleClock::new(now);
        let mut cfg = FastTailConfig {
            lock_enabled: true,
            screensaver_timeout_mins: 10,
            ..FastTailConfig::default()
        };
        let later = now + Duration::from_secs(600);
        assert!(!clock.idle_lock_due(later, &cfg), "no PIN, no lock");
        cfg.lock_pin = scramble_pin("4821");
        assert!(clock.idle_lock_due(later, &cfg));
        assert!(!clock.idle_lock_due(later - Duration::from_secs(1), &cfg));
        clock.touch(now + Duration::from_secs(300));
        assert!(
            !clock.idle_lock_due(later, &cfg),
            "input restarts the clock"
        );
        cfg.lock_enabled = false;
        assert!(!clock.idle_lock_due(later + later.duration_since(now), &cfg));
        cfg.lock_enabled = true;
        cfg.screensaver_timeout_mins = 0;
        assert!(
            !idle_lock_armed(&cfg),
            "zero minutes turns the idle lock off"
        );
    }
}
