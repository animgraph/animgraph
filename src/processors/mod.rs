pub mod animation_pose;
pub mod blend_in;
pub mod blend_ranges;
pub mod conditional_tree;
pub mod event_emitted;
pub mod inactive_layer;
pub mod pose_mask;
pub mod reference_pose;
pub mod speed_scale;
pub mod state_event;
pub mod timed_event;
pub mod transform_offset;
pub mod tree;

#[cfg(all(test, feature = "compiler"))]
mod tests;

use std::fmt::Debug;
use std::ops::Range;

use crate::io::Event;
use crate::state_machine::NodeIndex;
use crate::{
    FlowStatus, Graph, GraphDefinition, GraphNodeRegistry, Id, IndexType, InterpreterContext,
};

use self::animation_pose::AnimationPoseNode;
use self::blend_in::BlendInNode;
use self::blend_ranges::BlendRangeNode;
use self::conditional_tree::ConditionalTreeNode;
use self::event_emitted::EventEmittedNode;
use self::inactive_layer::InactiveLayerNode;
use self::pose_mask::PoseMaskNode;
use self::reference_pose::ReferencePoseNode;
use self::speed_scale::SpeedScaleNode;
use self::state_event::StateEventNode;
use self::timed_event::TimedEventNode;
use self::transform_offset::TransformOffsetNode;
use self::tree::TreeNode;

pub fn add_default_constructors(meta: &mut GraphNodeRegistry) {
    meta.register::<TreeNode>();
    meta.register::<TransformOffsetNode>();
    meta.register::<ConditionalTreeNode>();
    meta.register::<AnimationPoseNode>();
    meta.register::<BlendInNode>();
    meta.register::<BlendRangeNode>();
    meta.register::<InactiveLayerNode>();
    meta.register::<PoseMaskNode>();
    meta.register::<ReferencePoseNode>();
    meta.register::<SpeedScaleNode>();
    meta.register::<StateEventNode>();
    meta.register::<TimedEventNode>();
    meta.register::<EventEmittedNode>();
}

pub fn default_node_constructors() -> GraphNodeRegistry {
    let mut registry = GraphNodeRegistry::default();
    add_default_constructors(&mut registry);
    registry
}

#[cfg(feature = "compiler")]
pub mod compile {
    use crate::compiler::context::NodeCompilationRegistry;

    pub use super::animation_pose::compile::*;
    pub use super::blend_in::compile::*;
    pub use super::blend_ranges::compile::*;
    pub use super::conditional_tree::compile::*;
    pub use super::event_emitted::compile::*;
    pub use super::inactive_layer::compile::*;
    pub use super::pose_mask::compile::*;
    pub use super::reference_pose::compile::*;
    pub use super::speed_scale::compile::*;
    pub use super::state_event::compile::*;
    pub use super::timed_event::compile::*;
    pub use super::transform_offset::compile::*;
    pub use super::tree::compile::*;

    pub fn add_default_nodes(meta: &mut NodeCompilationRegistry) {
        meta.register::<TreeNode>();
        meta.register::<TransformOffsetNode>();
        meta.register::<ConditionalTreeNode>();
        meta.register::<AnimationPoseNode>();
        meta.register::<BlendInNode>();
        meta.register::<BlendRangeNode>();
        meta.register::<InactiveLayerNode>();
        meta.register::<PoseMaskNode>();
        meta.register::<ReferencePoseNode>();
        meta.register::<SpeedScaleNode>();
        meta.register::<StateEventNode>();
        meta.register::<TimedEventNode>();
        meta.register::<EventEmittedNode>();
    }

    pub fn default_compilation_nodes() -> NodeCompilationRegistry {
        let mut registry = NodeCompilationRegistry::default();
        add_default_nodes(&mut registry);
        registry
    }
}

pub trait GraphNode: Debug + 'static + Sync + Send {
    fn visit(&self, visitor: &mut GraphVisitor);
}

#[derive(Default, Clone)]
pub struct FlowEvents {
    pub emitted: Vec<Id>,
}

impl FlowEvents {
    pub fn clear(&mut self) {
        self.emitted.clear();
    }

    pub fn push(&mut self, definition: &GraphDefinition, index: Event) {
        self.emitted.push(definition.get_event_id(index));
    }
}

pub struct GraphVisitor<'a> {
    pub graph: &'a mut Graph,
    pub definition: &'a GraphDefinition,
    pub events: &'a mut FlowEvents,
    pub context: InterpreterContext,
    pub status: FlowStatus,
    pub current: NodeIndex,
}

impl<'a> GraphVisitor<'a> {
    pub fn new(
        graph: &'a mut Graph,
        definition: &'a GraphDefinition,
        events: &'a mut FlowEvents,
        context: InterpreterContext,
        status: FlowStatus,
    ) -> Self {
        Self {
            graph,
            context,
            definition,
            status,
            events,
            current: NodeIndex(0),
        }
    }

    pub fn emit_event(&mut self, event: Event) {
        self.events.push(self.definition, event);
    }

    pub fn visit(&mut self, children: Range<IndexType>) {
        debug_assert!(self.current.0 < children.start);
        if let Some(items) = self.definition.get_nodes(children.clone()) {
            for (node, index) in items.iter().zip(children) {
                self.current = NodeIndex(index);
                node.visit_node(self);
            }
        }
    }

    pub fn visit_node(&mut self, node_index: NodeIndex) {
        if let Some(node) = self.definition.get_node(node_index) {
            self.current = node_index;
            node.visit_node(self);
        }
    }
}
