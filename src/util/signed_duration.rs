use std::ops::{Add, Sub};
use std::time::{Duration, SystemTime};

/// A wrapper around [Duration] taht supports both positive and negative
/// durations.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SignedDuration {
    NonNegative(Duration),
    Negative(Duration),
}

impl SignedDuration {
    /// Returns the difference between two [SystemTime]s that may be before or
    /// after one another.
    pub(crate) fn difference(time1: SystemTime, time2: SystemTime) -> Self {
        match time1.duration_since(time2) {
            Ok(duration) => SignedDuration::NonNegative(duration),
            Err(error) => SignedDuration::Negative(error.duration()),
        }
    }
}

impl Add<SignedDuration> for SystemTime {
    type Output = SystemTime;

    fn add(self, duration: SignedDuration) -> SystemTime {
        match duration {
            SignedDuration::NonNegative(duration) => self + duration,
            SignedDuration::Negative(duration) => self - duration,
        }
    }
}

impl Sub<SignedDuration> for SystemTime {
    type Output = SystemTime;

    fn sub(self, duration: SignedDuration) -> SystemTime {
        match duration {
            SignedDuration::NonNegative(duration) => self - duration,
            SignedDuration::Negative(duration) => self + duration,
        }
    }
}
