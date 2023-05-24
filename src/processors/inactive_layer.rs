use serde_derive::{Deserialize, Serialize};

use crate::{GraphNodeConstructor, LayerWeight};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InactiveLayerNode;

impl GraphNodeConstructor for InactiveLayerNode {
    fn identity() -> &'static str {
        "inactive_layer"
    }

    fn construct_entry(
        self,
        _metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl GraphNode for InactiveLayerNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        visitor.context.layer_weight = LayerWeight::ZERO;
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_json::Value;

    use crate::{
        compiler::prelude::{Extras, IOType, Node, NodeCompiler, NodeSettings},
        GraphNodeConstructor,
    };

    pub use super::InactiveLayerNode;

    #[derive(Default, Debug, Clone)]
    pub struct InactiveLayerSettings;

    impl NodeSettings for InactiveLayerSettings {
        fn name() -> &'static str {
            InactiveLayerNode::identity()
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

    impl NodeCompiler for InactiveLayerNode {
        type Settings = InactiveLayerSettings;

        fn build<'a>(
            context: &crate::compiler::prelude::NodeSerializationContext<'a>,
        ) -> Result<Value, crate::compiler::prelude::NodeCompilationError> {
            context.serialize_node(InactiveLayerNode)
        }
    }

    pub fn inactive_layer() -> Node {
        Node::new_disconnected(InactiveLayerSettings).expect("Valid")
    }
}
