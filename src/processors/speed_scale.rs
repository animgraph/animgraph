use serde_derive::{Deserialize, Serialize};

use crate::{io::NumberRef, Alpha, GraphNodeConstructor};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SpeedScaleNode {
    pub scale: NumberRef<f32>,
    pub blend: NumberRef<Alpha>,
}

impl GraphNodeConstructor for SpeedScaleNode {
    fn identity() -> &'static str {
        "speed_scale"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_number_ref(&self.scale, Self::identity())?;
        metrics.validate_number_ref(&self.blend, Self::identity())?;
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl GraphNode for SpeedScaleNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        let mut factor = self.scale.get(visitor.graph);
        let blend = self.blend.get(visitor.graph);
        factor = blend.lerp(1.0, factor);
        visitor.context.apply_time_scale(factor);
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_json::Value;

    use crate::{
        compiler::prelude::{
            Extras, IOSlot, IOType, Node, NodeCompilationError, NodeCompiler,
            NodeSerializationContext, NodeSettings,
        },
        Alpha, GraphNodeConstructor,
    };

    pub use super::SpeedScaleNode;

    #[derive(Default, Debug, Clone)]
    pub struct SpeedScaleSettings;

    impl NodeSettings for SpeedScaleSettings {
        fn name() -> &'static str {
            SpeedScaleNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[("scale", Number), ("blend", Number)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(Extras::default())
        }
    }

    impl NodeCompiler for SpeedScaleNode {
        type Settings = SpeedScaleSettings;

        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            let scale = context.input_number(0)?;
            let blend = context.input_number(1)?;

            context.serialize_node(SpeedScaleNode { scale, blend })
        }
    }

    pub fn speed_scale(scale: impl IOSlot<f32>, blend: impl IOSlot<Alpha>) -> Node {
        let settings = SpeedScaleSettings;
        let input = vec![scale.into_slot("scale"), blend.into_slot("blend")];
        Node::new(input, [], settings).expect("Valid")
    }
}
