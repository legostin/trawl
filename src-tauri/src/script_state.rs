//! In-memory state shared across script runs (counter()/once()/everyNth()).
//! Lives for the app session only — resets on restart. Dry-run gets a fresh
//! instance per invocation so tests never mutate the real counters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Default)]
pub struct ScriptState {
    counters: Mutex<HashMap<String, i64>>,
}

impl ScriptState {
    /// Increment the named counter and return the new value (first call → 1).
    pub fn bump(&self, name: &str) -> i64 {
        let mut counters = self.counters.lock().unwrap();
        let value = counters.entry(name.to_string()).or_insert(0);
        *value += 1;
        *value
    }

    /// Remove the named counter so the next bump starts from 1 again.
    pub fn reset(&self, name: &str) {
        self.counters.lock().unwrap().remove(name);
    }
}

/// The app-wide store used by real proxy traffic.
pub fn global() -> Arc<ScriptState> {
    static GLOBAL: OnceLock<Arc<ScriptState>> = OnceLock::new();
    GLOBAL.get_or_init(|| Arc::new(ScriptState::default())).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_increments_from_one() {
        let state = ScriptState::default();
        assert_eq!(state.bump("a"), 1);
        assert_eq!(state.bump("a"), 2);
        assert_eq!(state.bump("a"), 3);
    }

    #[test]
    fn reset_starts_over() {
        let state = ScriptState::default();
        state.bump("a");
        state.bump("a");
        state.reset("a");
        assert_eq!(state.bump("a"), 1);
    }

    #[test]
    fn names_are_independent() {
        let state = ScriptState::default();
        assert_eq!(state.bump("a"), 1);
        assert_eq!(state.bump("b"), 1);
        assert_eq!(state.bump("a"), 2);
    }

    #[test]
    fn instances_do_not_share() {
        let a = ScriptState::default();
        let b = ScriptState::default();
        a.bump("x");
        assert_eq!(b.bump("x"), 1);
    }

    #[test]
    fn global_is_a_singleton() {
        global().bump("__test_singleton");
        assert_eq!(global().bump("__test_singleton"), 2);
        global().reset("__test_singleton");
    }
}
