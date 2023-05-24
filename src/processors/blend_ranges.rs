use std::ops::Range;

use serde_derive::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    core::{remap_unorm, Alpha, ALPHA_ZERO},
    io::NumberRef,
    state_machine::NodeIndex,
    BlendSampleId, BlendTree, Graph, GraphNodeConstructor, IndexType, PoseGraph, PoseNode,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendRangeNode {
    pub stops: Vec<f32>,
    pub input: NumberRef<f32>,
    pub children: Range<IndexType>,
}

#[derive(Error, Debug)]
pub enum BlendRangeError {
    #[error("unordered blend range")]
    UnorderedBlendRange,
    #[error("blend range stops not matching child count")]
    UnexpectedBlendRangeCount,
}

impl GraphNodeConstructor for BlendRangeNode {
    fn identity() -> &'static str {
        "blend_range"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_number_ref(&self.input, Self::identity())?;
        metrics.validate_children(&self.children, Self::identity())?;

        if self.stops.len() != self.children.len() {
            return Err(BlendRangeError::UnexpectedBlendRangeCount.into());
        }

        for (a, b) in self.stops.iter().zip(self.stops.iter().skip(1)) {
            if *a > *b {
                return Err(BlendRangeError::UnorderedBlendRange.into());
            }
        }

        Ok(crate::GraphNodeEntry::Pose(Box::new(self)))
    }
}

impl BlendRangeNode {
    fn get_blend(&self, x: f32) -> (usize, usize, Alpha) {
        if self.stops.is_empty() {
            return (0, 0, ALPHA_ZERO);
        }

        let (index, end) = 'first: {
            for (index, &end) in self.stops.iter().enumerate() {
                if x <= end {
                    if index == 0 || x == end {
                        return (index, index, ALPHA_ZERO);
                    }
                    break 'first (index, end);
                }
            }

            let last = self.stops.len() - 1;
            return (last, last, ALPHA_ZERO);
        };

        let start = self.stops[index - 1];
        if x == start {
            return (index - 1, index - 1, ALPHA_ZERO);
        }

        let w = remap_unorm(x, start..=end);
        (index - 1, index, w)
    }
}

impl GraphNode for BlendRangeNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        visitor.visit(self.children.clone());
    }
}

impl PoseNode for BlendRangeNode {
    fn sample(
        &self,
        tasks: &mut BlendTree,
        graph: &Graph,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        if self.children.is_empty() {
            return Ok(None);
        }

        let x = self.input.get(graph);
        let (a, b, w) = self.get_blend(x);

        if b >= self.children.len() {
            return Ok(None);
        }

        let task_a = graph.sample_pose(tasks, NodeIndex(self.children.start + a as IndexType))?;
        if a == b {
            return Ok(task_a);
        }

        let task_b = graph.sample_pose(tasks, NodeIndex(self.children.start + b as IndexType))?;

        Ok(match (task_a, task_b) {
            (Some(a), Some(b)) => Some(tasks.blend_masked(a, b, w)),
            (Some(a), None) => Some(a),
            (None, res) => res,
        })
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_derive::{Deserialize, Serialize};
    use serde_json::Value;

    use crate::{
        compiler::prelude::{
            Extras, IOSlot, IOType, Node, NodeCompilationError, NodeCompiler,
            NodeSerializationContext, NodeSettings, DEFAULT_INPUT_NAME,
        },
        model::NodeChildRange,
        GraphNodeConstructor, IndexType,
    };

    pub use super::BlendRangeNode;

    #[derive(Default, Debug, Clone, Serialize, Deserialize)]
    pub struct BlendRangeSettings {
        pub blend_range_stops: Vec<f32>,
    }

    impl NodeSettings for BlendRangeSettings {
        fn name() -> &'static str {
            BlendRangeNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_INPUT_NAME, Number)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(serde_json::to_value(self)?)
        }

        fn child_range() -> NodeChildRange {
            NodeChildRange::UpTo(IndexType::MAX as usize)
        }
    }

    impl NodeCompiler for BlendRangeNode {
        type Settings = BlendRangeSettings;

        fn build<'a>(
            context: &NodeSerializationContext<'a>,
        ) -> Result<Value, NodeCompilationError> {
            let input = context.input_number(0)?;

            let settings = context.settings::<BlendRangeSettings>()?;

            context.serialize_node(BlendRangeNode {
                input,
                stops: settings.blend_range_stops,
                children: context.children(),
            })
        }
    }

    pub fn blend_ranges(input: impl IOSlot<f32>, ranges: impl Into<Vec<(f32, Node)>>) -> Node {
        let nodes: Vec<(f32, Node)> = ranges.into();
        let stops: Vec<f32> = nodes.iter().map(|x| x.0).collect();
        let settings = BlendRangeSettings {
            blend_range_stops: stops,
        };
        let input = vec![input.into_slot(DEFAULT_INPUT_NAME)];
        let children: Vec<Node> = nodes.into_iter().map(|x| x.1).collect();

        Node::new(input, children, settings).expect("Valid")
    }
}
