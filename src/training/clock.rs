//! Fischer game clocks and the one-extra-iteration search policy.
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct FischerControl {
    pub initial_ms: u64,
    pub increment_ms: u64,
}
impl FischerControl {
    pub fn validate(self) -> Result<(), String> {
        if self.initial_ms <= 1000
            || self.increment_ms == 0
            || self.initial_ms > 86_400_000
            || self.increment_ms > 3_600_000
        {
            return Err("Fischer clock requires initial time > 1000ms (at most one day), and increment 1..3600000ms".into());
        }
        Ok(())
    }
    pub fn budget(self, remaining_ms: u64) -> MoveBudget {
        // Always retain one second; a depleted bank gets an immediate fallback move.
        let reserve = 1000;
        let hard_ms = remaining_ms.saturating_sub(reserve);
        MoveBudget {
            hard_ms,
            soft_ms: self.increment_ms.min(hard_ms),
        }
    }
    pub fn finish_move(self, remaining_ms: u64, elapsed_ms: u64) -> Option<u64> {
        // No increment rescues a flag: award it only after an on-time legal move.
        (elapsed_ms < remaining_ms).then(|| remaining_ms - elapsed_ms + self.increment_ms)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MoveBudget {
    pub hard_ms: u64,
    pub soft_ms: u64,
}

impl MoveBudget {
    /// hard_ms is the original bank minus the one-second reserve, before caps.
    fn bank_ms(self) -> u64 { self.hard_ms.saturating_add(1000) }
    pub fn admission_ms(self) -> u64 {
        (5000u64.saturating_add(self.bank_ms() / 100)).min(self.hard_ms)
    }
    pub fn search_hard_ms(self) -> u64 {
        (5000u64.saturating_add(self.bank_ms() / 10)).min(self.hard_ms)
    }
}

/// Called only between completed iterations. The extension is spent when the
/// next depth would not fit in the sustainable budget, even if it finishes fast.
#[derive(Default)]
pub(crate) struct IterationBudget {
    extended: bool,
}
impl IterationBudget {
    pub fn start_next(&mut self, last: Duration, elapsed: Duration, budget: MoveBudget) -> bool {
        if self.extended {
            return false;
        }
        let estimate = last.saturating_mul(3);
        let hard = Duration::from_millis(budget.admission_ms()).saturating_sub(elapsed);
        if estimate > hard {
            return false;
        }
        let soft = Duration::from_millis(budget.soft_ms).saturating_sub(elapsed);
        if estimate > soft {
            self.extended = true;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClockMove {
    pub elapsed_ms: u64,
    pub remaining_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClockRecord {
    pub control: FischerControl,
    /// One entry per move record, in the same order. Remaining time includes increment.
    pub moves: Vec<ClockMove>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag_fell: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }
    #[test]
    fn one_extension_even_when_prediction_is_conservative() {
        let mut p = IterationBudget::default();
        let b = MoveBudget {
            soft_ms: 5000,
            hard_ms: 899000,
        };
        assert!(p.start_next(ms(500), ms(700), b));
        assert!(p.start_next(ms(1800), ms(2500), b)); // next depth crosses soft budget
        assert!(!p.start_next(ms(100), ms(2600), b)); // never a second extension
    }
    #[test]
    fn bank_scaled_limits_and_threefold_admission() {
        let clock = FischerControl { initial_ms: 900000, increment_ms: 5000 };
        for (remaining, admission, hard) in [
            (900000, 14000, 95000), (300000, 8000, 35000),
            (60000, 5600, 11000), (10000, 5100, 6000),
            (6000, 5000, 5000), (800, 0, 0),
        ] {
            let budget = clock.budget(remaining);
            assert_eq!(budget.admission_ms(), admission);
            assert_eq!(budget.search_hard_ms(), hard);
        }
        let budget = clock.budget(300000);
        assert!(IterationBudget::default().start_next(ms(2000), ms(2000), budget));
        assert!(!IterationBudget::default().start_next(ms(2000), ms(2001), budget));
        assert!(!IterationBudget::default().start_next(ms(3000), ms(0), budget));
    }

    #[test]
    fn low_clock_preserves_bank_and_rejects_unaffordable_depth() {
        let c = FischerControl {
            initial_ms: 900000,
            increment_ms: 5000,
        };
        assert_eq!(c.budget(9000).hard_ms, 8000);
        let mut p = IterationBudget::default();
        assert!(!p.start_next(ms(3000), ms(2500), c.budget(9000)));
        assert!(p.start_next(ms(500), ms(1000), c.budget(9000)));
        assert_eq!(c.budget(800).hard_ms, 0);
        assert_eq!(c.finish_move(900000, 7000), Some(898000));
        assert_eq!(c.finish_move(1000, 1000), None);
        assert_eq!(c.finish_move(1000, 1001), None);
    }
}
