use serde_derive::{Deserialize, Serialize};

use crate::{
    io::Resource, BlendSampleId, BlendTree, BoneGroup, Graph, GraphNodeConstructor, PoseNode,
    PoseParent,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoseMaskNode {
    pub bone_group: Resource<BoneGroup>,
}

impl GraphNodeConstructor for PoseMaskNode {
    fn identity() -> &'static str {
        "pose_mask"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_resource(&self.bone_group, Self::identity())?;
        Ok(crate::GraphNodeEntry::PoseParent(Box::new(self)))
    }
}

impl GraphNode for PoseMaskNode {
    fn visit(&self, _visitor: &mut GraphVisitor) {}
}

impl PoseParent for PoseMaskNode {
    fn apply_parent(
        &self,
        tasks: &mut BlendTree,
        child_task: BlendSampleId,
        graph: &Graph,
    ) -> anyhow::Result<BlendSampleId> {
        if let Some(res) = self.bone_group.get(graph) {
            Ok(tasks.apply_mask(child_task, res.group))
        } else {
            Ok(child_task)
        }
    }

    fn as_pose(&self) -> Option<&dyn PoseNode> {
        None
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
        io::Resource,
        model::{IOSlot, DEFAULT_INPUT_NAME},
        BoneGroup, GraphNodeConstructor,
    };

    pub use super::PoseMaskNode;

    #[derive(Default, Debug, Clone)]
    pub struct PoseMaskNodeSettings;

    impl NodeSettings for PoseMaskNodeSettings {
        fn name() -> &'static str {
            PoseMaskNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            &[(DEFAULT_INPUT_NAME, BoneGroup::IO_TYPE)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(Extras::default())
        }
    }

    impl NodeCompiler for PoseMaskNode {
        type Settings = PoseMaskNodeSettings;

        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            let bone_group = context.input_resource(0)?;
            context.serialize_node(PoseMaskNode { bone_group })
        }
    }

    pub fn pose_mask(bone_group: impl IOSlot<Resource<BoneGroup>>) -> Node {
        let input = vec![bone_group.into_slot(DEFAULT_INPUT_NAME)];
        Node::new(input, vec![], PoseMaskNodeSettings).expect("Valid")
    }
}
