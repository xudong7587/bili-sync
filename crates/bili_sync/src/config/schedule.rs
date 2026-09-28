use anyhow::{Result, ensure};
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RefreshSchedule {
    pub start: String,
    pub end: String,
    pub jitter_min_seconds: u64,
    pub jitter_max_seconds: u64,
}

impl RefreshSchedule {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.start.is_empty() == self.end.is_empty(),
            "刷新时间段的开始和结束需同时填写"
        );
        if !self.start.is_empty() {
            NaiveTime::parse_from_str(&self.start, "%H:%M")?;
            NaiveTime::parse_from_str(&self.end, "%H:%M")?;
        }
        ensure!(
            self.jitter_min_seconds <= self.jitter_max_seconds && self.jitter_max_seconds <= 86400,
            "随机间隙需满足 0 ≤ 最小值 ≤ 最大值 ≤ 86400 秒"
        );
        Ok(())
    }

    pub fn allows(&self, time: NaiveTime) -> bool {
        if self.start.is_empty() && self.end.is_empty() {
            return true;
        }
        let (Ok(start), Ok(end)) = (
            NaiveTime::parse_from_str(&self.start, "%H:%M"),
            NaiveTime::parse_from_str(&self.end, "%H:%M"),
        ) else {
            return false;
        };
        if start == end {
            true
        } else if start < end {
            time >= start && time < end
        } else {
            time >= start || time < end
        }
    }

    pub fn delay_seconds(&self, trigger: &super::Trigger) -> u64 {
        if matches!(trigger, super::Trigger::Cron(_)) {
            return 0;
        }
        let min = self.jitter_min_seconds.min(86400);
        let max = self.jitter_max_seconds.clamp(min, 86400);
        min + (uuid::Uuid::new_v4().as_u128() % u128::from(max - min + 1)) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(value: &str) -> NaiveTime {
        NaiveTime::parse_from_str(value, "%H:%M").unwrap()
    }
    #[test]
    fn overnight_window_includes_start_excludes_end() {
        let schedule = RefreshSchedule {
            start: "22:00".into(),
            end: "07:00".into(),
            ..Default::default()
        };
        assert!(schedule.allows(time("22:00")));
        assert!(schedule.allows(time("06:59")));
        assert!(!schedule.allows(time("07:00")));
        assert!(!schedule.allows(time("12:00")));
    }
    #[test]
    fn legacy_defaults_allow_all_day_without_delay() {
        let schedule: RefreshSchedule = serde_json::from_str("{}").unwrap();
        assert!(schedule.allows(time("12:00")));
        assert_eq!(schedule.delay_seconds(&super::super::Trigger::default()), 0);
        assert!(schedule.validate().is_ok());
    }
    #[test]
    fn cron_ignores_saved_jitter_but_interval_keeps_it() {
        let schedule = RefreshSchedule {
            jitter_min_seconds: 30,
            jitter_max_seconds: 90,
            ..Default::default()
        };
        let cron = super::super::Trigger::Cron("0 0 2 * * *".into());
        let interval = super::super::Trigger::Interval(1200);
        for _ in 0..100 {
            assert_eq!(schedule.delay_seconds(&cron), 0);
            assert!((30..=90).contains(&schedule.delay_seconds(&interval)));
        }
        // Switching to Cron must not erase the values used on switching back.
        assert_eq!(schedule.jitter_min_seconds, 30);
        assert_eq!(schedule.jitter_max_seconds, 90);
    }

    #[test]
    fn rejects_partial_window_and_inverted_jitter() {
        assert!(
            RefreshSchedule {
                start: "09:00".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            RefreshSchedule {
                jitter_min_seconds: 60,
                jitter_max_seconds: 10,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
