use serde_derive::{Deserialize, Serialize};

use crate::{
    io::{Resource, Timer},
    AnimationClip, BlendSampleId, BlendTree, FlowStatus, Graph, GraphMetrics, GraphNodeConstructor,
    GraphNodeEntry, PoseNode,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationPoseNode {
    pub clip: Resource<AnimationClip>,
    pub timer: Timer,
}

impl GraphNodeConstructor for AnimationPoseNode {
    fn identity() -> &'static str {
        "animation_pose"
    }

    fn construct_entry(self, metrics: &GraphMetrics) -> anyhow::Result<GraphNodeEntry> {
        metrics.validate_timer(&self.timer, Self::identity())?;
        metrics.validate_resource(&self.clip, Self::identity())?;
        Ok(GraphNodeEntry::Pose(Box::new(self)))
    }
}

impl GraphNode for AnimationPoseNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        if visitor.status == FlowStatus::Initialized {
            if let Some(clip) = self.clip.get(visitor.graph) {
                self.timer.init(visitor.graph, clip.init_timer());
            }
        } else {
            let _ = self.timer.tick(visitor.graph, visitor.context.delta_time());
        }
    }
}

impl PoseNode for AnimationPoseNode {
    fn sample(
        &self,
        tasks: &mut BlendTree,
        graph: &Graph,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        if let Some(clip) = self.clip.get(graph) {
            let sample = tasks.sample_animation_clip(clip.animation, self.timer.time(graph));

            Ok(Some(tasks.apply_mask(sample, clip.bone_group)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_json::Value;

    use crate::{
        compiler::{
            context::NodeCompiler,
            prelude::{
                Extras, IOSlot, IOType, Node, NodeCompilationError, NodeSerializationContext,
                NodeSettings, DEFAULT_INPUT_NAME, DEFAULT_OUPTUT_NAME,
            },
        },
        io::Resource,
        GraphNodeConstructor,
    };

    use super::AnimationClip;

    pub use super::AnimationPoseNode;

    #[derive(Default, Debug, Clone)]
    pub struct AnimationPoseSettings;

    impl NodeSettings for AnimationPoseSettings {
        fn name() -> &'static str {
            AnimationPoseNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            &[(DEFAULT_INPUT_NAME, AnimationClip::IO_TYPE)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_OUPTUT_NAME, Timer)]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(Extras::default())
        }
    }

    impl NodeCompiler for AnimationPoseNode {
        type Settings = AnimationPoseSettings;

        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            let clip = context.input_resource(0)?;
            let timer = context.output_timer(0)?;

            context.serialize_node(AnimationPoseNode { clip, timer })
        }
    }

    pub fn animation_pose(clip: impl IOSlot<Resource<AnimationClip>>) -> Node {
        let input = vec![clip.into_slot(DEFAULT_INPUT_NAME)];
        Node::new(input, vec![], AnimationPoseSettings).expect("Valid")
    }
}
