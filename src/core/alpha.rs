use std::ops::{Add, Mul, MulAssign, RangeInclusive};

use serde_derive::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
/// [0,1]
pub struct Alpha(pub f32);

pub const ALPHA_ZERO: Alpha = Alpha(0.0);
pub const ALPHA_ONE: Alpha = Alpha(1.0);

pub const ALPHA_NEARLY_ZERO: Alpha = Alpha(1e-6);
pub const ALPHA_NEARLY_ONE: Alpha = Alpha(1.0 - ALPHA_NEARLY_ZERO.0);

impl Alpha {
    pub fn is_nearly_zero(self) -> bool {
        self.0 <= ALPHA_NEARLY_ZERO.0
    }

    pub fn is_nearly_one(self) -> bool {
        self.0 >= ALPHA_NEARLY_ONE.0
    }

    pub fn lerp<T: Add<Output = T> + Mul<f32, Output = T>>(self, a: T, b: T) -> T {
        a * self.inverse().0 + b * self.0
    }
}

impl PartialEq for Alpha {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() <= ALPHA_NEARLY_ZERO.0
    }
}

impl Eq for Alpha {}

impl PartialOrd for Alpha {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.total_cmp(&other.0))
    }
}

impl Ord for Alpha {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl Mul for Alpha {
    type Output = Alpha;

    fn mul(self, rhs: Self) -> Self::Output {
        Alpha(self.0 * rhs.0)
    }
}

impl MulAssign for Alpha {
    fn mul_assign(&mut self, rhs: Self) {
        self.0 *= rhs.0;
    }
}

impl Alpha {
    pub fn inverse(self) -> Self {
        Self(1.0 - self.0)
    }

    pub fn interpolate(self, a: Alpha, b: Alpha) -> Alpha {
        Alpha((a * self.inverse()).0 + (b * self).0)
    }
}

pub fn unorm_clamped(x: f32) -> Alpha {
    Alpha(x.clamp(0.0, 1.0))
}

pub fn remap_unorm(x: f32, range: RangeInclusive<f32>) -> Alpha {
    let d = range.end() - range.start();
    if d > 0.0 {
        let t = (x - range.start()) / d;
        Alpha(t.clamp(0.0, 1.0))
    } else {
        ALPHA_ZERO
    }
}

pub fn unorm_wrapped(x: f32) -> Alpha {
    if x < 0.0 {
        Alpha(1.0 + x.fract())
    } else if x >= 1.0 {
        Alpha(x.fract())
    } else {
        Alpha(x.abs())
    }
}

pub fn unorm_clamped_64(x: f64) -> Alpha {
    Alpha(x.clamp(0.0, 1.0) as f32)
}

pub fn unorm_wrapped_64(x: f64) -> Alpha {
    let n = if x < 0.0 {
        1.0 + x.fract()
    } else if x > 1.0 {
        x.fract()
    } else {
        x.abs()
    };

    Alpha(n as f32)
}
