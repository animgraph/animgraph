use std::num::NonZeroU16;
use std::sync::Arc;
pub type IndexType = u16;
pub type OptionalIndexType = Option<NonZeroU16>;

#[cfg(feature = "compiler")]
pub mod compiler;

mod blend;
mod core;
mod data;
mod graph;
mod graph_definition;
mod interpreter;
mod layer_builder;

pub mod io;
pub mod processors;
pub mod state_machine;

pub use crate::blend::*;
pub use crate::core::*;
pub use crate::data::*;
pub use crate::graph::*;
pub use crate::graph_definition::*;
pub use crate::interpreter::*;
pub use crate::layer_builder::*;

pub use anyhow;
pub use serde;
pub use serde_derive;
pub use serde_json;
pub use glam;
#[cfg(feature = "compiler")]
pub use uuid;

use processors::FlowEvents;

#[derive(Default, Clone)]
pub struct DefaultRunContext {
    pub events: FlowEvents,
    pub layers: LayerBuilder,
    pub tree: BlendTree,
    pub delta_time: f64,
}

impl DefaultRunContext {
    pub fn new(delta_time: f64) -> Self {
        DefaultRunContext {
            events: Default::default(),
            layers: Default::default(),
            tree: Default::default(),
            delta_time,
        }
    }

    pub fn run_without_blend(&mut self, graph: &mut Graph) {
        let defintion = Arc::clone(graph.definition());
        let _runner = Interpreter::run(
            graph,
            &defintion,
            &mut self.events,
            &mut self.layers,
            self.delta_time,
        );
    }

    pub fn run_and_append(&mut self, graph: &mut Graph) {
        self.run_without_blend(graph);
        self.tree.append(graph, &self.layers).expect("Valid blend");
    }

    pub fn run(&mut self, graph: &mut Graph) {
        self.clear();
        self.run_and_append(graph);
    }

    pub fn clear(&mut self) {
        self.tree.clear();
        self.layers.clear();
        self.events.clear();
    }
}
