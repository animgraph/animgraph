use serde_derive::{Deserialize, Serialize};

use crate::{
    io::{Event, EventEmit},
    GraphBoolean, GraphNodeConstructor,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct StateEventNode {
    pub event: Event,
    pub condition: GraphBoolean,
    pub emit: EventEmit,
}

impl GraphNodeConstructor for StateEventNode {
    fn identity() -> &'static str {
        "state_event"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_event(&self.event, Self::identity())?;
        metrics.validate_bool(self.condition, Self::identity())?;
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl GraphNode for StateEventNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        let state = visitor.graph.get_state(visitor.context.state);
        if visitor.graph.get_bool(self.condition) {
            self.event.emit(visitor, state, self.emit);
        } else {
            self.event.emit(visitor, state, EventEmit::Never);
        }
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
        io::{Event, EventEmit},
    };

    pub use super::StateEventNode;

    #[derive(Default, Debug, Clone, Serialize, Deserialize)]
    pub struct StateEventSettings {
        emit: EventEmit,
    }

    impl NodeSettings for StateEventSettings {
        fn name() -> &'static str {
            "state_event"
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[("event", Event), ("condition", Bool)]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(serde_json::to_value(self)?)
        }
    }

    impl NodeCompiler for StateEventNode {
        type Settings = StateEventSettings;
        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            let event = context.input_event(0)?;
            let condition = context.input_bool(1)?;
            let settings = context.settings::<StateEventSettings>()?;

            context.serialize_node(StateEventNode {
                event,
                condition,
                emit: settings.emit,
            })
        }
    }

    pub fn state_event(
        event: impl IOSlot<Event>,
        condition: impl IOSlot<bool>,
        emit: EventEmit,
    ) -> Node {
        let settings = StateEventSettings { emit };
        let input = vec![event.into_slot("event"), condition.into_slot("condition")];
        Node::new(input, vec![], settings).expect("Valid")
    }
}
