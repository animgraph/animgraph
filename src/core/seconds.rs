use std::ops::{Add, AddAssign, Mul, MulAssign};

use serde_derive::{Deserialize, Serialize};

use super::alpha::{unorm_wrapped, Alpha, ALPHA_ZERO};

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seconds(pub f32);

impl From<f32> for Seconds {
    fn from(value: f32) -> Self {
        Seconds(value)
    }
}

impl From<Seconds> for f32 {
    fn from(value: Seconds) -> Self {
        value.0
    }
}

impl Mul<f32> for Seconds {
    type Output = Seconds;

    fn mul(self, rhs: f32) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl MulAssign<f32> for Seconds {
    fn mul_assign(&mut self, rhs: f32) {
        self.0 *= rhs;
    }
}

impl Seconds {
    pub fn normalized_offset_looping(&self, start: Seconds) -> Alpha {
        if self.0 == 0.0 {
            ALPHA_ZERO
        } else {
            unorm_wrapped(start.0 / self.0)
        }
    }

    pub fn is_nearly_zero(&self) -> bool {
        self.0.abs() < 1e-6
    }
}

impl Eq for Seconds {}

impl PartialOrd for Seconds {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.total_cmp(&other.0))
    }
}

impl Ord for Seconds {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl Add for Seconds {
    type Output = Seconds;

    fn add(self, rhs: Self) -> Self::Output {
        Seconds(self.0 + rhs.0)
    }
}

impl AddAssign for Seconds {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}
