use serde_derive::{Deserialize, Serialize};

use crate::{
    core::{unorm_clamped, Seconds, Alpha, ALPHA_ONE},
    io::{NumberMut, NumberRef},
    FlowStatus, Graph, GraphNodeConstructor,
};

use super::{GraphNode, GraphVisitor};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct BlendInNode {
    pub init: NumberRef<Alpha>,
    pub duration: NumberRef<Seconds>,
    pub result: NumberMut<Alpha>,
}

impl GraphNodeConstructor for BlendInNode {
    fn identity() -> &'static str {
        "blend_in"
    }

    fn construct_entry(self, metrics: &crate::GraphMetrics) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_number_ref(&self.init, Self::identity())?;
        metrics.validate_number_ref(&self.duration, Self::identity())?;
        metrics.validate_number_mut(&self.result, Self::identity())?;
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl BlendInNode {
    fn update(&self, graph: &mut Graph, delta_time: Seconds) {
        let current = self.result.get(graph);
        if current == ALPHA_ONE {
            return;
        }

        let duration = self.duration.get(graph);
        if duration.is_nearly_zero() {
            self.result.set(graph, ALPHA_ONE);
            return;
        }

        let x = delta_time.0 / duration.0;
        self.result.set(graph, unorm_clamped(current.0 + x));
    }
}

impl GraphNode for BlendInNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        if visitor.status == FlowStatus::Initialized {
            let value = self.init.get(visitor.graph);
            self.result.set(visitor.graph, value)
        } else {
            self.update(visitor.graph, visitor.context.delta_time())
        }
    }
}

#[cfg(feature = "compiler")]
pub mod compile {
    use serde_json::Value;

    use crate::{
        compiler::prelude::{
            Extras, IOSlot, IOType, Node, NodeCompiler, NodeSerializationContext, NodeCompilationError,
            NodeSettings, DEFAULT_OUPTUT_NAME,
        },
        core::{Seconds, Alpha},
        GraphNodeConstructor,
    };

    pub use super::BlendInNode;

    #[derive(Default, Debug, Clone)]
    pub struct BlendInSettings;

    impl NodeSettings for BlendInSettings {
        fn name() -> &'static str {
            BlendInNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[("init", Number), ("duration", Number)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_OUPTUT_NAME, Number)]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(Extras::default())
        }
    }

    impl NodeCompiler for BlendInNode {
        type Settings = BlendInSettings;

        fn build<'a>(context: &NodeSerializationContext<'a>) -> Result<Value, NodeCompilationError> {
            let init = context.input_number(0)?;
            let duration = context.input_number(1)?;
            let result = context.output_number(0)?;
            context.serialize_node(BlendInNode {
                init,
                duration,
                result,
            })
        }
    }

    pub fn blend_in(init: impl IOSlot<Alpha>, duration: impl IOSlot<Seconds>) -> Node {
        let settings = BlendInSettings;
        let input = vec![init.into_slot("init"), duration.into_slot("duration")];
        Node::new(input, vec![], settings).expect("Valid")
    }
}
