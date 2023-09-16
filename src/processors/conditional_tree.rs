use serde_derive::{Deserialize, Serialize};

use crate::{
    io::BoolMut, BlendSampleId, BlendTree, FlowStatus, Graph, GraphBoolean, GraphNodeConstructor,
    PoseNode, PoseParent,
};

use super::{GraphNode, GraphVisitor, TreeNode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalTreeNode {
    pub condition: GraphBoolean,
    pub result: BoolMut,
    pub tree: TreeNode,
}

impl GraphNodeConstructor for ConditionalTreeNode {
    fn identity() -> &'static str {
        "conditional_tree"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_bool_mut(&self.result, Self::identity())?;
        metrics.validate_bool(self.condition, Self::identity())?;
        metrics.validate_children(&self.tree.children, Self::identity())?;
        Ok(crate::GraphNodeEntry::PoseParent(Box::new(self)))
    }
}

impl Default for ConditionalTreeNode {
    fn default() -> Self {
        Self {
            condition: GraphBoolean::Always,
            result: BoolMut::default(),
            tree: TreeNode::default(),
        }
    }
}

impl GraphNode for ConditionalTreeNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        let condition = if visitor.status == FlowStatus::Initialized {
            let condition = visitor.graph.get_bool(self.condition);
            self.result.set(visitor.graph, condition);
            condition
        } else {
            self.result.get(visitor.graph)
        };

        if condition {
            self.tree.visit(visitor);
        }
    }
}

impl PoseParent for ConditionalTreeNode {
    fn apply_parent(
        &self,
        tasks: &mut BlendTree,
        child_task: BlendSampleId,
        graph: &Graph,
    ) -> anyhow::Result<BlendSampleId> {
        if self.result.get(graph) {
            self.tree.apply_parent(tasks, child_task, graph)
        } else {
            Ok(child_task)
        }
    }

    fn as_pose(&self) -> Option<&dyn PoseNode> {
        Some(self)
    }
}

impl PoseNode for ConditionalTreeNode {
    fn sample(
        &self,
        tasks: &mut BlendTree,
        graph: &Graph,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        if self.result.get(graph) {
            self.tree.sample(tasks, graph)
        } else {
            Ok(None)
        }
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
        model::{IOSlot, NodeChildRange, DEFAULT_INPUT_NAME, DEFAULT_OUPTUT_NAME},
        processors::compile::TreeNode,
        GraphNodeConstructor, IndexType,
    };

    pub use super::ConditionalTreeNode;

    #[derive(Default, Debug, Clone)]
    pub struct ConditionalTreeSettings;

    impl NodeSettings for ConditionalTreeSettings {
        fn name() -> &'static str {
            ConditionalTreeNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            &[(DEFAULT_INPUT_NAME, IOType::Bool)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[(DEFAULT_OUPTUT_NAME, IOType::Bool)]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(Extras::default())
        }

        fn child_range() -> NodeChildRange {
            NodeChildRange::UpTo(IndexType::MAX as usize)
        }
    }

    impl NodeCompiler for ConditionalTreeNode {
        type Settings = ConditionalTreeSettings;

        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            context.serialize_node(ConditionalTreeNode {
                condition: context.input_bool(0)?,
                result: context.output_bool(0)?,
                tree: TreeNode {
                    children: context.children(),
                },
            })
        }
    }

    pub fn conditional_tree(condition: impl IOSlot<bool>, nodes: impl Into<Vec<Node>>) -> Node {
        Node::new(
            vec![condition.into_slot(DEFAULT_INPUT_NAME)],
            nodes.into(),
            ConditionalTreeSettings,
        )
        .expect("Valid")
    }

    #[cfg(test)]
    #[test]
    fn test_conditional_tree() {
        use crate::compiler::prelude::*;
        use std::sync::Arc;

        let state_machines = [state_machine(
            "Root",
            [state("Reference").with(
                endpoint(conditional_tree(
                    bind_parameter::<bool>("a"),
                    [reference_pose()],
                )),
                [],
            )],
        )];

        let graph = AnimGraph {
            state_machines: state_machines.into(),
            ..Default::default()
        };

        let compiled =
            GraphDefinitionCompilation::compile(&graph, &default_compilation_nodes()).unwrap();
        let definition = compiled
            .builder
            .build(&default_node_constructors())
            .unwrap();
        let a = definition.get_bool_parameter("a").unwrap();

        let mut graph = definition.build_with_empty_skeleton(Arc::new(EmptyResourceProvider));

        let mut context = DefaultRunContext::new(1.0);

        context.run(&mut graph);

        assert!(context.tree.is_empty());

        graph.reset_graph();
        a.set(&mut graph, true);

        context.run_without_blend(&mut graph);
        context.tree.clear();
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(
            context.tree.append(&graph, &context.layers).unwrap(),
            Some(BlendSampleId::Task(123))
        );
    }
}
