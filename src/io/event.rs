use serde_derive::{Deserialize, Serialize};

use crate::{processors::GraphVisitor, FlowState, Graph, IndexType};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventEmit {
    #[default]
    Never,
    Always,
    Entry,
    Exit,
    Active,
    Or(FlowState, FlowState),
    Changed(FlowState),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Event(pub IndexType);

impl From<IndexType> for Event {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

impl From<Event> for usize {
    fn from(value: Event) -> Self {
        value.0 as usize
    }
}
impl Event {
    pub fn get(&self, graph: &Graph) -> FlowState {
        graph.get_event(*self)
    }

    pub fn set(&self, graph: &mut Graph, state: FlowState) -> FlowState {
        graph.set_event(*self, state)
    }

    pub fn emit(&self, visitor: &mut GraphVisitor, state: FlowState, emit: EventEmit) {
        let previous = visitor.graph.set_event(*self, state);
        let send = match emit {
            EventEmit::Never => false,
            EventEmit::Always => true,
            EventEmit::Entry => !previous.is_active() && state.is_active(),
            EventEmit::Exit => previous.is_active() && !state.is_active(),
            EventEmit::Active => state.is_active(),
            EventEmit::Or(x, y) => state == x || state == y,
            EventEmit::Changed(on_state) => previous != on_state && state == on_state,
        };

        if send {
            visitor.emit_event(*self);
        }
    }
}
