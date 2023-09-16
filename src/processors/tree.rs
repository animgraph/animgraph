use std::ops::Range;

use serde_derive::{Deserialize, Serialize};

use crate::{
    state_machine::NodeIndex, BlendSampleId, BlendTree, Graph, GraphNodeConstructor, IndexType,
    PoseGraph, PoseNode, PoseParent,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TreeNode {
    pub children: Range<IndexType>,
}

impl GraphNodeConstructor for TreeNode {
    fn identity() -> &'static str {
        "tree"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_children(&self.children, Self::identity())?;
        Ok(crate::GraphNodeEntry::PoseParent(Box::new(self)))
    }
}

impl GraphNode for TreeNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        visitor.visit(self.children.clone());
    }
}

impl PoseParent for TreeNode {
    fn apply_parent(
        &self,
        tasks: &mut BlendTree,
        mut child_task: BlendSampleId,
        graph: &Graph,
    ) -> anyhow::Result<BlendSampleId> {
        for child in self.children.clone() {
            if let Some(parent) = graph.pose_parent(NodeIndex(child)) {
                child_task = parent.apply_parent(tasks, child_task, graph)?;
            }
        }

        Ok(child_task)
    }

    fn as_pose(&self) -> Option<&dyn PoseNode> {
        Some(self)
    }
}

impl PoseNode for TreeNode {
    fn sample(
        &self,
        tasks: &mut BlendTree,
        graph: &Graph,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        for index in self.children.clone() {
            if let Some(mut child_task) = graph.sample_pose(tasks, NodeIndex(index))? {
                for sibling in index + 1..self.children.end {
                    if let Some(parent) = graph.pose_parent(NodeIndex(sibling)) {
                        child_task = parent.apply_parent(tasks, child_task, graph)?;
                    }
                }
                return Ok(Some(child_task));
            }
        }
        Ok(None)
    }
}

#[cfg(feature = "compiler")]
pub mod compile {

    use serde_json::Value;

    use crate::{
        compiler::prelude::{
            Extras, IOType, Node, NodeCompilationError, NodeCompiler, NodeSerializationContext,
            NodeSettings,
        },
        model::NodeChildRange,
        GraphNodeConstructor, IndexType,
    };

    pub use super::TreeNode;

    #[derive(Debug, Clone, Default)]
    pub struct TreeSettings;

    impl NodeSettings for TreeSettings {
        fn name() -> &'static str {
            TreeNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(Extras::default())
        }

        fn child_range() -> NodeChildRange {
            NodeChildRange::UpTo(IndexType::MAX as usize)
        }
    }

    impl NodeCompiler for TreeNode {
        type Settings = TreeSettings;

        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            context.serialize_node(TreeNode {
                children: context.children(),
            })
        }
    }

    pub fn tree(nodes: impl Into<Vec<Node>>) -> Node {
        Node::new(vec![], nodes.into(), TreeSettings).expect("Valid")
    }
}
