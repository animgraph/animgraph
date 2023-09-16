//! Example showing how to construct, serialize, compile, instantiate and run graphs with compiler global constants

use animgraph::compiler::prelude::*;
use std::sync::Arc;

const TEST_EVENT: &str = "TestEvent";

fn main() -> anyhow::Result<()> {
    // Custom compiler global name
    const ALMOST_PI: &str = "ALMOST_PI";

    fn construct_data() -> AnimGraph {
        fn global_number(name: &str) -> Expression {
            Expression::CompilerGlobal(name.to_owned())
        }

        let condition = bind_parameter::<f32>("a").ge(global_number(ALMOST_PI));
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

        // Add a custom compiler global
        let mut definition = GraphDefinitionCompilation::default();
        definition.add_global_constant(ALMOST_PI, 3.0)?;

        let mut context = GraphCompilationContext::build_context(graph, &registry)?;
        run_graph_definition_compilation(&mut definition, &mut context)?;
        Ok(definition.builder)
    }

    // 1. Serialize
    fn serialize_definition() -> serde_json::Value {
        let animgraph = construct_data();
        let definition_builder = compile_data(&animgraph).unwrap();
        serde_json::to_value(definition_builder).unwrap()
    }

    // 2. Deserialize
    fn deserialize_definition(
        serialized: serde_json::Value,
    ) -> anyhow::Result<Arc<GraphDefinition>> {
        let deserialized: GraphDefinitionBuilder = serde_json::from_value(serialized)?;
        // Validates the graph and deserializes the immutable nodes
        let mut provider = GraphNodeRegistry::default();
        add_default_constructors(&mut provider);
        Ok(deserialized.build(&provider)?)
    }

    // This is an example. Serialization is not necessary
    let serialized = serialize_definition();
    let definition = deserialize_definition(serialized)?;

    perform_runtime_test(definition);

    Ok(())
}

fn perform_runtime_test(definition: Arc<GraphDefinition>) {
    // 3. Create the graph
    let mut graph = definition
        .clone()
        .build_with_empty_skeleton(Arc::new(EmptyResourceProvider));

    // 4. Query the graph
    let event = definition.get_event_by_name(TEST_EVENT).unwrap();
    assert!(event.get(&graph) == FlowState::Exited);

    // 5. Run the graph
    let mut context = DefaultRunContext::new(1.0);
    context.run(&mut graph);
    assert!(context.events.emitted.is_empty());

    assert!(event.get(&graph) == FlowState::Entered);

    // 6. Modify parameters
    let a = definition.get_number_parameter::<f32>("a").unwrap();
    a.set(&mut graph, 4.0);

    context.run(&mut graph);
    assert_eq!(&context.events.emitted, &[Id::from_str(TEST_EVENT)]);
}
