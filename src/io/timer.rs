use serde_derive::{Deserialize, Serialize};

use crate::{
    core::{SampleTimer, Seconds, Alpha},
    Graph, IndexType,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Timer(pub IndexType);

impl From<IndexType> for Timer {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

impl From<Timer> for usize {
    fn from(value: Timer) -> Self {
        value.0 as usize
    }
}

impl Timer {
    pub fn init(&self, graph: &mut Graph, init: SampleTimer) {
        graph.with_timer_mut(self.0, |timer| *timer = init);
    }

    pub fn tick(&self, graph: &mut Graph, delta_time: Seconds) -> SampleTimer {
        graph
            .with_timer_mut(self.0, |timer| {
                timer.tick(delta_time);
                *timer
            })
            .expect("Valid timer")
    }

    pub fn time(&self, graph: &Graph) -> Alpha {
        graph
            .with_timer(self.0, |timer| timer.time())
            .expect("Valid timer")
    }

    pub fn remaining(&self, graph: &Graph) -> Seconds {
        graph
            .with_timer(self.0, |timer| timer.remaining())
            .expect("Valid timer")
    }

    pub fn elapsed(&self, graph: &Graph) -> Seconds {
        graph
            .with_timer(self.0, |timer| timer.elapsed())
            .expect("Valid timer")
    }
}
