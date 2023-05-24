use std::num::NonZeroU32;

use super::{unorm_clamped, unorm_wrapped, Alpha, Seconds, ALPHA_ONE, ALPHA_ZERO};

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct SampleRange(pub Alpha, pub Alpha);

impl SampleRange {
    pub fn inverse(self) -> Self {
        Self(self.1.inverse(), self.0.inverse())
    }

    pub fn ordered(&self) -> (Alpha, Alpha) {
        if self.0 .0 <= self.1 .0 {
            (self.0, self.1)
        } else {
            (self.1, self.0)
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
#[cfg_attr(
    feature = "compiler",
    derive(serde_derive::Serialize, serde_derive::Deserialize),
    serde(rename_all = "snake_case")
)]
pub struct SampleTime {
    pub t0: Alpha,
    pub t1: Alpha,
}

impl From<Alpha> for SampleTime {
    fn from(value: Alpha) -> Self {
        Self {
            t0: value,
            t1: value,
        }
    }
}

impl SampleTime {
    pub const ZERO: SampleTime = SampleTime {
        t0: ALPHA_ZERO,
        t1: ALPHA_ZERO,
    };

    pub fn is_looping(&self) -> bool {
        self.t0.0 > self.t1.0
    }

    #[inline]
    pub fn samples(&self) -> (SampleRange, Option<SampleRange>) {
        if self.is_looping() {
            (
                SampleRange(self.t0, ALPHA_ONE),
                Some(SampleRange(ALPHA_ZERO, self.t1)),
            )
        } else {
            (SampleRange(self.t0, self.t1), None)
        }
    }

    pub fn time(&self) -> Alpha {
        self.t1
    }

    pub fn step_clamped(&mut self, x: f32) {
        let t0 = self.time();
        let t1 = unorm_clamped(t0.0 + x);

        self.t0 = t0;
        self.t1 = t1;
    }

    pub fn step_looping(&mut self, x: f32) {
        let t0 = self.time();
        let t1 = unorm_wrapped(t0.0 + x);

        self.t0 = t0;
        self.t1 = t1;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "compiler",
    derive(serde_derive::Serialize, serde_derive::Deserialize),
    serde(rename_all = "snake_case")
)]
pub struct SampleTimer {
    pub sample_time: SampleTime,
    pub duration: Seconds,
    pub looping: Option<NonZeroU32>,
    // TODO: probably add pub periods: Option<NonZeroU32> here,
}

impl SampleTimer {
    pub const FIXED: SampleTimer = SampleTimer {
        sample_time: SampleTime::ZERO,
        duration: Seconds(0.0),
        looping: None,
    };

    pub fn new(start: Seconds, duration: Seconds, looping: bool) -> SampleTimer {
        let inital_time = duration.normalized_offset_looping(start);
        Self {
            sample_time: SampleTime {
                t0: inital_time,
                t1: inital_time,
            },
            duration,
            looping: if looping { NonZeroU32::new(1) } else { None },
        }
    }

    pub fn is_fixed(&self) -> bool {
        self.duration.0 == 0.0
    }

    pub fn is_looping(&self) -> bool {
        self.looping.is_some()
    }

    pub fn set_looping(&mut self, value: bool) {
        if value {
            self.looping = NonZeroU32::new(1);
        } else {
            self.looping = None;
        }
    }

    pub fn tick(&mut self, delta_time: Seconds) {
        if !self.is_fixed() {
            let x = delta_time.0 / self.duration.0;
            if let Some(iteration) = self.looping.as_mut() {
                // TODO: should this handle x > 1.0?
                let was_looping = self.sample_time.is_looping();
                self.sample_time.step_looping(x);
                if !was_looping && self.sample_time.is_looping() {
                    *iteration = iteration.saturating_add(1);
                }
            } else {
                self.sample_time.step_clamped(x);
            }
        }
    }

    pub fn time(&self) -> Alpha {
        self.sample_time.time()
    }

    pub fn remaining(&self) -> Seconds {
        self.duration * self.time().inverse().0
    }

    pub fn elapsed(&self) -> Seconds {
        self.duration * self.time().0
    }
}
