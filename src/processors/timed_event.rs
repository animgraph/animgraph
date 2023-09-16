use serde_derive::{Deserialize, Serialize};

use crate::{
    io::{Event, EventEmit, NumberRef, Timer},
    FlowState, GraphBoolean, GraphNodeConstructor, Seconds,
};

use super::{GraphNode, GraphVisitor};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct TimedEventNode {
    pub timer: Timer,
    pub event: Event,
    pub condition: GraphBoolean,
    pub duration: NumberRef<Seconds>,
    pub emit: EventEmit,
    pub elapsed: bool,
}

impl GraphNodeConstructor for TimedEventNode {
    fn identity() -> &'static str {
        "timed_event"
    }

    fn construct_entry(
        self,
        metrics: &crate::GraphMetrics,
    ) -> anyhow::Result<crate::GraphNodeEntry> {
        metrics.validate_timer(&self.timer, Self::identity())?;
        metrics.validate_event(&self.event, Self::identity())?;
        metrics.validate_bool(self.condition, Self::identity())?;
        metrics.validate_number_ref(&self.duration, Self::identity())?;
        Ok(crate::GraphNodeEntry::Process(Box::new(self)))
    }
}

impl GraphNode for TimedEventNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        let emit = if visitor.graph.get_bool(self.condition) {
            self.emit
        } else {
            EventEmit::Never
        };

        let duration = self.duration.get(visitor.graph);

        let in_event = if self.elapsed {
            self.timer.elapsed(visitor.graph).0 >= duration.0
        } else {
            self.timer.remaining(visitor.graph).0 <= duration.0
        };

        let state = if in_event {
            visitor.graph.get_state(visitor.context.state)
        } else {
            FlowState::Exited
        };

        self.event.emit(visitor, state, emit);
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
        io::{Event, EventEmit, Timer},
        GraphNodeConstructor, Seconds,
    };

    pub use super::TimedEventNode;

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct TimedEventSettings {
        pub emit: EventEmit,
        pub elapsed: bool,
    }

    impl NodeSettings for TimedEventSettings {
        fn name() -> &'static str {
            TimedEventNode::identity()
        }

        fn input() -> &'static [(&'static str, IOType)] {
            use IOType::*;
            &[
                ("timer", Timer),
                ("event", Event),
                ("condition", Bool),
                ("duration", Number),
            ]
        }

        fn output() -> &'static [(&'static str, IOType)] {
            &[]
        }

        fn build(self) -> anyhow::Result<Extras> {
            Ok(serde_json::to_value(self)?)
        }
    }

    impl NodeCompiler for TimedEventNode {
        type Settings = TimedEventSettings;
        fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
            let timer = context.input_timer(0)?;
            let event = context.input_event(1)?;
            let condition = context.input_bool(2)?;
            let duration = context.input_number::<Seconds>(3)?;
            let settings = context.settings::<TimedEventSettings>()?;

            context.serialize_node(TimedEventNode {
                timer,
                event,
                condition,
                duration,
                emit: settings.emit,
                elapsed: settings.elapsed,
            })
        }
    }

    pub fn remaining_event(
        timer: impl IOSlot<Timer>,
        event: impl IOSlot<Event>,
        condition: impl IOSlot<bool>,
        duration: impl IOSlot<Seconds>,
        emit: EventEmit,
    ) -> Node {
        let settings = TimedEventSettings {
            emit,
            elapsed: false,
        };
        let input = vec![
            timer.into_slot("timer"),
            event.into_slot("event"),
            condition.into_slot("condition"),
            duration.into_slot("duration"),
        ];
        Node::new(input, vec![], settings).expect("Valid")
    }
}
