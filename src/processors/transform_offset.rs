use glam::Vec3A;
use serde_derive::{Deserialize, Serialize};

use crate::{
    io::{VectorMut, VectorRef},
    GraphNodeConstructor, Id,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformOffsetNode {
    pub bone: Id,
    pub offset: VectorRef,
    pub result: VectorMut,
}

impl GraphNodeConstructor for TransformOffsetNode {
    fn identity() -> &'static str {
        "transform_offset"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_vector_ref(&self.offset, Self::identity())?;
        metrics.validate_vector_mut(&self.result, Self::identity())?;
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl GraphNode for TransformOffsetNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        let offset = self.offset.get(visitor.graph);
        let result = if let Some(transform) = visitor.graph.transform_by_id(self.bone) {
            (Vec3A::from(offset) - transform.translation).into()
        } else {
            offset
        };

        self.result.set(visitor.graph, result);
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_derive::{Deserialize, Serialize};
    use serde_json::Value;

    use crate::{
        compiler::prelude::{
            Extras, IOSlot, IOType, Node, NodeCompilationError, NodeCompiler,
            NodeSerializationContext, NodeSettings, DEFAULT_OUPTUT_NAME,
        },
        model::DEFAULT_INPUT_NAME,
        GraphNodeConstructor, Id,
    };

    pub use super::TransformOffsetNode;

    #[derive(Debug, Clone, Deserialize, Serialize, Default)]
    pub struct TransformTargetSettings {
        pub bone: String,
    }

    impl NodeSettings for TransformTargetSettings {
        fn name() -> &'static str {
            TransformOffsetNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_INPUT_NAME, Vector)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_OUPTUT_NAME, Vector)]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(serde_json::to_value(self)?)
        }
    }

    impl NodeCompiler for TransformOffsetNode {
        type Settings = TransformTargetSettings;

        fn build<'a>(
            context: &NodeSerializationContext<'a>,
        ) -> Result<Value, NodeCompilationError> {
            let settings = context.settings::<Self::Settings>()?;
            let offset = context.input_vector(0)?;
            let result = context.output_vector(0)?;
            context.serialize_node(TransformOffsetNode {
                bone: Id::from_str(&settings.bone),
                offset,
                result,
            })
        }
    }

    pub fn transform_offset(bone: &str, offset: impl IOSlot<[f32; 3]>) -> Node {
        // TODO: How to notify of misspellings?
        let settings = TransformTargetSettings {
            bone: bone.to_owned(),
        };
        let input = vec![offset.into_slot(DEFAULT_INPUT_NAME)];
        Node::new(input, vec![], settings).expect("Valid")
    }

    #[cfg(test)]
    #[test]
    fn test_transform_offset() {
        use glam::{Affine3A, Vec3};

        use crate::compiler::prelude::*;
        use std::sync::Arc;

        const ROOT_BONE: &str = "Root";

        const ROOT_ID: Id = Id::from_str(ROOT_BONE);
        let skeleton = Skeleton {
            id: SkeletonId(0),
            bones: vec![ROOT_ID],
            parents: vec![],
        };

        let node = alias(
            "root_offset",
            transform_offset(ROOT_BONE, bind_parameter("point_of_interest")),
        );

        let state_machines = [state_machine(
            "Root",
            [
                state("Base")
                    .with_branch(endpoint(node))
                    .with_transitions([contains_inclusive(
                        (1.0, 1.5),
                        bind_route::<[f32; 3]>("root_offset").projection(Projection::Length),
                    )
                    .immediate_transition("Target")]),
                state("Target").with_branch(endpoint(reference_pose())),
            ],
        )];

        let anim_graph = AnimGraph {
            state_machines: state_machines.into(),
            ..Default::default()
        };

        let compiled =
            GraphDefinitionCompilation::compile(&anim_graph, &default_compilation_nodes()).unwrap();
        let definition = compiled
            .builder
            .build(&default_node_constructors())
            .unwrap();
        let vec = definition
            .get_vector_parameter("point_of_interest")
            .unwrap();
        let mut context = DefaultRunContext::new(1.0);
        let range = 1.0..=1.5;

        let mut graph = definition.clone().build_with_empty_skeleton(Arc::new(EmptyResourceProvider));
        for test in [0f32, 0.99, 1.0, 1.3, 1.5, 2.0] {
            graph.reset_graph();
            context.run(&mut graph);
            vec.set(&mut graph, [0.0, test, 0.0]);
            context.clear();
            context.run_without_blend(&mut graph);
            context
                .tree
                .set_reference_task(Some(BlendSampleId::Task(123)));
            assert_eq!(
                context
                    .tree
                    .append(&graph, &context.layers)
                    .unwrap()
                    .is_some(),
                range.contains(&test),
                "Unexpected result for {test}"
            );
        }

        let mut graph =
            definition.build(Arc::new(EmptyResourceProvider), Arc::new(skeleton));

        let translation = Vec3::new(0.1, 0.2, 0.3);
        for test in [0f32, 0.99, 1.0, 1.3, 1.5, 2.0] {
            graph.reset_graph();
            graph.set_transforms(
                [Affine3A::from_translation(translation).into()],
                &Default::default(),
            );
            context.run(&mut graph);
            vec.set(&mut graph, [0.0, test, 0.0]);
            context.clear();
            context.run_without_blend(&mut graph);
            context
                .tree
                .set_reference_task(Some(BlendSampleId::Task(123)));

            let x = (Vec3::new(0.0, test, 0.0) - translation).length();
            assert_eq!(
                context
                    .tree
                    .append(&graph, &context.layers)
                    .unwrap()
                    .is_some(),
                range.contains(&x),
                "Unexpected result for {test}"
            );
        }
    }
}
