use serde_derive::{Deserialize, Serialize};

use crate::{BlendSampleId, BlendTree, Graph, GraphNodeConstructor, PoseNode};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferencePoseNode;

impl GraphNodeConstructor for ReferencePoseNode {
    fn identity() -> &'static str {
        "reference_pose"
    }

    fn construct_entry(
        self,
        _metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        Ok(crate::GraphNodeEntry::Pose(Box::new(self)))
    }
}

impl GraphNode for ReferencePoseNode {
    fn visit(&self, _visitor: &mut GraphVisitor) {}
}

impl PoseNode for ReferencePoseNode {
    fn sample(
        &self,
        tasks: &mut BlendTree,
        _graph: &Graph,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        Ok(tasks.get_reference_task())
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_json::Value;

    use crate::{
        compiler::{
            context::NodeCompiler,
            prelude::{
                Extras, IOType, Node, NodeCompilationError, NodeSerializationContext, NodeSettings,
            },
        },
        GraphNodeConstructor,
    };

    pub use super::ReferencePoseNode;

    #[derive(Default, Debug, Clone)]
    pub struct ReferencePoseSettings;

    impl NodeSettings for ReferencePoseSettings {
        fn name() -> &'static str {
            ReferencePoseNode::identity()
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
    }

    impl NodeCompiler for ReferencePoseNode {
        type Settings = ReferencePoseSettings;

        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            context.serialize_node(ReferencePoseNode)
        }
    }

    pub fn reference_pose() -> Node {
        Node::new(vec![], vec![], ReferencePoseSettings).expect("Valid")
    }

    #[cfg(test)]
    #[test]
    fn test_reference_pose() {
        use crate::compiler::prelude::*;
        use std::sync::Arc;

        let state_machines = [state_machine(
            "Root",
            [
                state("Reference").with(
                    endpoint(reference_pose()),
                    [bind_parameter::<bool>("a")
                        .into_expr()
                        .immediate_transition("Target")],
                ),
                state("Target"),
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
        let a = definition.get_bool_parameter("a").unwrap();

        let mut graph = definition.build_with_empty_skeleton(Arc::new(EmptyResourceProvider));
        let mut context = DefaultRunContext::new(1.0);
        context.run_without_blend(&mut graph);
        assert!(context.tree.is_empty());
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(
            context.tree.append(&graph, &context.layers).unwrap(),
            Some(BlendSampleId::Task(123))
        );

        a.set(&mut graph, true);
        context.run_without_blend(&mut graph);
        context.tree.clear();
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(context.tree.append(&graph, &context.layers).unwrap(), None);
    }

    #[cfg(test)]
    #[test]
    fn test_reference_pose_layers() {
        use crate::compiler::prelude::*;
        use std::sync::Arc;
        fn create_clip_asset(name: &str, url: &str) -> ResourceContent {
            ResourceContent {
                name: name.to_owned(),
                resource_type: AnimationClip::RESOURCE_TYPE.to_owned(),
                content: Value::String(url.to_owned()),
                ..Default::default()
            }
        }

        let state_machines = [state_machine(
            "Root",
            [state("Layers").with_layers([
                endpoint(reference_pose()),
                endpoint(animation_pose("idle_looping")),
            ])],
        )];

        let anim_graph = AnimGraph {
            resources: [create_clip_asset("idle_looping", "character_idle.anim")].into(),
            state_machines: state_machines.into(),
            ..Default::default()
        };

        let compiled =
            GraphDefinitionCompilation::compile(&anim_graph, &default_compilation_nodes()).unwrap();
        let definition = compiled
            .builder
            .build(&default_node_constructors())
            .unwrap();

        let resources = Arc::new(
            SimpleResourceProvider::new_with_map(
                &definition,
                AnimationClip::default(),
                |_resource_type, _content| Ok(AnimationClip::default()),
            )
            .expect("Valid definition resources"),
        );

        let mut graph = definition.build_with_empty_skeleton(resources);
        let mut context = DefaultRunContext::new(1.0);
        context.run(&mut graph);
        assert_eq!(
            context.tree.get(),
            &[BlendSample::Animation {
                id: AnimationId(0),
                normalized_time: 0.0
            }],
        );

        context.clear();

        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        context.run_and_append(&mut graph);
        assert_eq!(
            context.tree.get(),
            &[
                BlendSample::Animation {
                    id: AnimationId(0),
                    normalized_time: 0.0
                },
                BlendSample::Blend(
                    BlendSampleId::Task(123),
                    BlendSampleId::Task(0),
                    1.0,
                    BoneGroupId::All
                )
            ],
        );
    }
}
