mod id;
mod math;
mod sample_timer;
mod seconds;
// mod sync_timer;
// mod snorm;
mod alpha;
mod transform;

pub use id::*;
pub use math::*;
pub use sample_timer::*;
pub use seconds::*;
pub use alpha::*;
pub use transform::*;

pub trait FromFloatUnchecked: 'static {
    fn from_f32(x: f32) -> Self;
    fn from_f64(x: f64) -> Self;
    fn into_f32(self) -> f32;
    fn into_f64(self) -> f64;
}

impl FromFloatUnchecked for Alpha {
    fn from_f32(x: f32) -> Self {
        Self(x)
    }
    fn from_f64(x: f64) -> Self {
        Self(x as _)
    }
    fn into_f32(self) -> f32 {
        self.0
    }

    fn into_f64(self) -> f64 {
        self.0 as f64
    }
}

impl FromFloatUnchecked for Seconds {
    fn from_f32(x: f32) -> Self {
        Self(x)
    }
    fn from_f64(x: f64) -> Self {
        Self(x as _)
    }
    fn into_f32(self) -> f32 {
        self.0
    }
    fn into_f64(self) -> f64 {
        self.0 as f64
    }
}

impl FromFloatUnchecked for f32 {
    fn from_f32(x: f32) -> Self {
        x
    }

    fn from_f64(x: f64) -> Self {
        x as Self
    }

    fn into_f32(self) -> f32 {
        self
    }

    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl FromFloatUnchecked for f64 {
    fn from_f32(x: f32) -> Self {
        x as Self
    }
    fn from_f64(x: f64) -> Self {
        x
    }

    fn into_f32(self) -> f32 {
        self as _
    }

    fn into_f64(self) -> f64 {
        self
    }
}
