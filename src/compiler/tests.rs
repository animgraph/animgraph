use std::sync::Arc;

use serde_derive::{Deserialize, Serialize};

use super::prelude::*;

#[test]
fn test_global_constant() {
    const TEST_EVENT: &str = "TestEvent";

    fn construct_data() -> AnimGraph {
        fn global_number(name: &str) -> Expression {
            Expression::CompilerGlobal(name.to_owned())
        }

        let condition = bind_parameter::<f32>("a").ge(global_number("ALMOST_PI"));
        let endpoint = endpoint(state_event(TEST_EVENT, condition, EventEmit::Always));

        let machines = [state_machine("Root", [state("StateA").with(endpoint, [])])];

        AnimGraph {
            state_machines: machines.into(),
            ..Default::default()
        }
    }

    fn compile_data(graph: &AnimGraph) -> Result<GraphDefinitionBuilder, CompileError> {
        let mut registry = NodeCompilationRegistry::default();
        add_default_nodes(&mut registry);

        let mut definition = GraphDefinitionCompilation::default();
        definition.add_global_constant("ALMOST_PI", 3.0)?;

        let mut context = GraphCompilationContext::build_context(graph, &registry)?;
        run_graph_definition_compilation(&mut definition, &mut context)?;
        Ok(definition.builder)
    }

    // 1. Serialize to disk
    fn serialize_definition() -> serde_json::Value {
        let animgraph = construct_data();
        let definition_builder = compile_data(&animgraph).unwrap();
        serde_json::to_value(definition_builder).unwrap()
    }

    // 2. Deserialize from disk
    fn deserialize_definition(serialized: serde_json::Value) -> Arc<GraphDefinition> {
        let deserialized: GraphDefinitionBuilder = serde_json::from_value(serialized).unwrap();
        // Validates the graph and deserializes the immutable nodes
        let mut provider = GraphNodeRegistry::default();
        add_default_constructors(&mut provider);
        deserialized.build(&provider).unwrap()
    }

    let serialized = serialize_definition();
    let definition = deserialize_definition(serialized);

    // 3. Run the graph
    let mut graph = definition
        .clone()
        .build_with_empty_skeleton(Arc::new(EmptyResourceProvider));

    let mut context = DefaultRunContext::new(1.0);

    // 4. Query the graph
    let event = definition.get_event_by_name(TEST_EVENT).unwrap();
    assert!(event.get(&graph) == FlowState::Exited);

    context.run(&mut graph);
    assert!(context.events.emitted.is_empty());

    assert!(event.get(&graph) == FlowState::Entered);

    // 5. Modify parameters
    let a = definition.get_number_parameter::<f32>("a").unwrap();
    a.set(&mut graph, 4.0);

    context.run(&mut graph);
    assert_eq!(&context.events.emitted, &[Id::from_str(TEST_EVENT)]);
}

#[test]
fn test_empty_machine_fails() {
    let machines = [state_machine("Root", [])];

    let graph = AnimGraph {
        state_machines: machines.into(),
        ..Default::default()
    };

    assert!(GraphDefinitionCompilation::compile(&graph, &default_compilation_nodes()).is_err());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultipleOutputsNode {
    first: Event,
    second: Timer,
    a: Event,
    b: Timer,
}

impl GraphNodeConstructor for MultipleOutputsNode {
    fn identity() -> &'static str {
        "test_multiple_outputs"
    }

    fn construct_entry(self, metrics: &GraphMetrics) -> anyhow::Result<GraphNodeEntry> {
        metrics.validate_event(&self.first, Self::identity())?;
        metrics.validate_timer(&self.second, Self::identity())?;
        metrics.validate_event(&self.a, Self::identity())?;
        metrics.validate_timer(&self.b, Self::identity())?;
        Ok(GraphNodeEntry::Process(Box::new(self.clone())))
    }
}

impl GraphNode for MultipleOutputsNode {
    fn visit(&self, _visitor: &mut GraphVisitor) {}
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct MultipleOutputSettings;

impl NodeSettings for MultipleOutputSettings {
    fn name() -> &'static str {
        MultipleOutputsNode::identity()
    }

    fn input() -> &'static [(&'static str, IOType)] {
        &[("first", IOType::Event), ("second", IOType::Timer)]
    }

    fn output() -> &'static [(&'static str, IOType)] {
        &[("A", IOType::Event), ("B", IOType::Timer)]
    }

    fn build(self) -> anyhow::Result<Extras> {
        Ok(Extras::default())
    }
}

impl NodeCompiler for MultipleOutputsNode {
    type Settings = MultipleOutputSettings;

    fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
        let first = context.input_event(0)?;
        let second = context.input_timer(1)?;
        let a = context.output_event(0)?;
        let b = context.output_timer(1)?;
        context.serialize_node(MultipleOutputsNode {
            first,
            second,
            a,
            b,
        })
    }
}

fn multiple_outputs(first: impl IOSlot<Event>, second: impl IOSlot<Timer>) -> Node {
    Node::new(
        [first.into_slot("first"), second.into_slot("second")],
        [],
        MultipleOutputSettings,
    )
    .expect("Valid")
}

#[test]
fn test_multiple_outputs() {
    fn construct_data() -> AnimGraph {
        let node1 = alias(
            "node1",
            multiple_outputs(bind_parameter("some_event"), bind_parameter("some_timer")),
        );

        let node2 = alias(
            "node2",
            multiple_outputs(bind_route("node1.A"), bind_route("node1.B")),
        );

        let node3 = alias(
            "node3",
            multiple_outputs(bind_route("node3.A"), bind_route("node3.B")),
        );

        let node4 = alias(
            "node4",
            multiple_outputs(bind_route("node5.A"), bind_route("node5.B")),
        );

        let node5 = alias(
            "node5",
            multiple_outputs(bind_route("node4.A"), bind_route("node4.B")),
        );

        let spaghetti = endpoint(tree([node1, node2, node3, node4, node5]));

        let machines = [state_machine("Root", [state("StateA").with(spaghetti, [])])];

        AnimGraph {
            state_machines: machines.into(),
            ..Default::default()
        }
    }

    fn compile_data(graph: &AnimGraph) -> Result<GraphDefinitionBuilder, CompileError> {
        let mut registry = NodeCompilationRegistry::default();
        registry.register::<TreeNode>();
        registry.register::<MultipleOutputsNode>();

        let mut definition = GraphDefinitionCompilation::default();
        definition.add_global_constant("ALMOST_PI", 3.0)?;

        let mut context = GraphCompilationContext::build_context(graph, &registry)?;
        run_graph_definition_compilation(&mut definition, &mut context)?;
        Ok(definition.builder)
    }

    let animgraph = construct_data();
    let _definition_builder = compile_data(&animgraph).unwrap();
}
