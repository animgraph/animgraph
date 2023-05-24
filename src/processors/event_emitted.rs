use serde_derive::{Deserialize, Serialize};

use crate::{
    io::{BoolMut, Event},
    GraphNodeConstructor,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct EventEmittedNode {
    pub event: Event,
    pub result: BoolMut,
}

impl GraphNodeConstructor for EventEmittedNode {
    fn identity() -> &'static str {
        "event_emitted"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_event(&self.event, Self::identity())?;
        metrics.validate_bool_mut(&self.result, Self::identity())?;
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl GraphNode for EventEmittedNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        let id = visitor.definition.get_event_id(self.event);
        let result = visitor.events.emitted.contains(&id);
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
            NodeSerializationContext, NodeSettings,
        },
        io::Event,
        model::{DEFAULT_INPUT_NAME, DEFAULT_OUPTUT_NAME},
        GraphNodeConstructor,
    };

    pub use super::EventEmittedNode;

    #[derive(Default, Debug, Clone, Serialize, Deserialize)]
    pub struct EventEmittedSettings;

    impl NodeSettings for EventEmittedSettings {
        fn name() -> &'static str {
            EventEmittedNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_INPUT_NAME, Event)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[(DEFAULT_OUPTUT_NAME, Bool)]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(serde_json::to_value(self)?)
        }
    }

    impl NodeCompiler for EventEmittedNode {
        type Settings = EventEmittedSettings;
        fn build<'a>(
            context: &NodeSerializationContext<'a>,
        ) -> Result<Value, NodeCompilationError> {
            let result = context.output_bool(0)?;
            let event = context.input_event(0)?;
            context.serialize_node(EventEmittedNode { event, result })
        }
    }

    pub fn event_emitted(event: impl IOSlot<Event>) -> Node {
        let settings = EventEmittedSettings;
        let input = vec![event.into_slot(DEFAULT_INPUT_NAME)];
        Node::new(input, vec![], settings).expect("Valid")
    }
}
