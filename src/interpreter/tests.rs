use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::sync::Arc;

use serde_derive::{Deserialize, Serialize};

use crate::processors::{FlowEvents, GraphNode, GraphVisitor};
use crate::state_machine::VariableIndex;

use crate::compiler::{prelude::*, resolve_routes, serialize_routes, GraphDefinitionCompilation};

fn collect_endpoints(
    builder: &LayerBuilder,
    index: usize,
    weight: Alpha,
    result: &mut Vec<(usize, Alpha)>,
) {
    let layer = &builder.layers[index];

    match &layer.layer_type {
        LayerType::Endpoint => {
            if let Some(node) = layer.node {
                result.push((node.0 as usize, weight));
            }
        }

        LayerType::List => {
            let mut previous = index;
            let mut next_child = layer.first_child;
            while let Some(index) = next_child {
                assert!(previous < index);

                let child = builder.layers.get(index).unwrap();

                let mut stack_weight = child.layer_weight() * weight;

                let mut next = child.next_sibling;
                previous = index;
                while let Some(index) = next {
                    assert!(previous < index, "{previous} {index}");

                    let sibling = builder.layers.get(index).unwrap();
                    stack_weight *= sibling.layer_weight().inverse();

                    previous = index;
                    next = sibling.next_sibling;
                }

                collect_endpoints(builder, index, stack_weight, result);

                previous = index;
                next_child = child.next_sibling;
            }
        }
        LayerType::StateMachine => {
            let mut previous = 0;
            let mut next_child = layer.first_child;
            while let Some(index) = next_child {
                assert!(previous < index);

                let child = builder.layers.get(index).unwrap();

                let mut stack_weight = child.transition_weight() * weight;

                let mut next = child.next_sibling;
                while let Some(index) = next {
                    assert!(previous < index);

                    let sibling = builder.layers.get(index).unwrap();
                    stack_weight *= sibling.transition_weight().inverse();

                    previous = index;
                    next = sibling.next_sibling;
                }

                collect_endpoints(builder, index, stack_weight, result);

                previous = index;
                next_child = child.next_sibling;
            }
        }
    }
}

pub enum Modified {
    Boolean(VariableIndex, bool),
    Number(VariableIndex, f32),
}

pub struct TestMachineEvalutor {
    pub data: Box<GraphDefinitionCompilation>,
    pub definition: Arc<GraphDefinition>,
    pub delta_time: f64,
    pub modified: Vec<Modified>,
}

fn serialize_and_compile(
    builder: GraphDefinitionBuilder,
) -> Result<Arc<GraphDefinition>, GraphBuilderError> {
    let serialized = serde_json::to_value(builder).unwrap();
    let builder: GraphDefinitionBuilder = serde_json::from_value(serialized).unwrap();
    let mut registry = GraphNodeRegistry::default();
    registry.register::<TestNode>();
    registry.register::<FixedTransitionNode>();
    builder.build(&registry)
}

impl TestMachineEvalutor {
    pub fn new(data: Box<GraphDefinitionCompilation>) -> Self {
        let definition = serialize_and_compile(data.builder.clone()).unwrap();

        Self {
            data,
            definition,
            delta_time: 1.0,
            modified: Vec::new(),
        }
    }

    pub fn tick(&mut self, graph: &mut Graph) -> f64 {
        let dt = if graph.iteration() == 0 {
            0.0
        } else {
            self.delta_time
        };

        for modified in self.modified.drain(..) {
            match modified {
                Modified::Boolean(index, value) => graph.set_variable_boolean(index, value),
                Modified::Number(index, value) => graph.set_variable_number(index, value),
            }
        }
        dt
    }

    pub fn set(&mut self, name: &str, input: &dyn std::any::Any) -> bool {
        if let Some(value) = input.downcast_ref::<bool>() {
            let index = self.data.get_boolean_parameter(name).expect("Boolean");
            self.modified.push(Modified::Boolean(index.index, *value));
        } else if let Some(value) = input.downcast_ref::<f64>() {
            let index = self.data.get_number_parameter(name).expect("Number");
            self.modified
                .push(Modified::Number(index.index, *value as _));
        } else {
            return false;
        };

        true
    }

    pub fn build_graph(&mut self) -> Result<Graph, GraphBuilderError> {
        let mut graph = self
            .definition
            .clone()
            .build_with_empty_skeleton(Arc::new(TestResourceProvider));
        graph.set_events_hook(Box::new(GraphHook(self.data.debug_triggers.clone())));
        Ok(graph)
    }
}

struct GraphHook(BTreeMap<IndexType, Value>);
impl FlowEventsHook for GraphHook {
    fn debug_trigger(
        &mut self,
        _graph: &Graph,
        _context: &InterpreterContext,
        debug_break: crate::graph::GraphDebugBreak,
        result: bool,
    ) {
        match debug_break {
            GraphDebugBreak::Condition { condition_index } => {
                if let Some(var) = self.0.get(&condition_index) {
                    println!("{}: {}", var, result);
                }
            }
        }
    }
}

pub struct TestResourceProvider;

impl GraphResourceProvider for TestResourceProvider {
    fn initialize(&self, entries: &mut [GraphResourceRef], _definition: &GraphDefinition) {
        assert!(entries.is_empty());
    }

    fn get(&self, _index: GraphResourceRef) -> &dyn std::any::Any {
        panic!("Should not be called by this tests. Otherwise implement the provider");
    }
}

impl TestMachineEvalutor {
    pub fn expect_endpoints(&mut self, steps: &[&[(&str, Alpha)]]) {
        let mut graph = self.build_graph().unwrap();
        self.check_flow_endpoints(steps, &mut graph);
    }

    pub fn check_flow_endpoints(&mut self, steps: &[&[(&str, Alpha)]], graph: &mut Graph) {
        let mut layers = LayerBuilder::default();
        let mut events = FlowEvents::default();
        let definition = graph.definition().clone();

        for expected_weighted in steps {
            layers.clear();
            let dt = self.tick(graph);

            let runner = Interpreter::run(graph, &definition, &mut events, &mut layers, dt);
            let step = runner.visitor.graph.iteration();

            let mut endpoints = Vec::new();
            collect_endpoints(runner.layers, 0, ALPHA_ONE, &mut endpoints);

            let result: Vec<_> = endpoints
                .into_iter()
                .map(|(index, weight)| {
                    (self.data.node_alias(NodeIndex(index as _)).unwrap(), weight)
                })
                .collect();
            assert_eq!(
                &result, expected_weighted,
                "Unexpected endpoints at step {step}\n{:#?}",
                &runner.layers.layers
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestNode;

impl GraphNodeConstructor for TestNode {
    fn identity() -> &'static str {
        "test"
    }

    fn construct_entry(self, _metrics: &GraphMetrics) -> anyhow::Result<GraphNodeEntry> {
        Ok(GraphNodeEntry::Process(Box::new(self.clone())))
    }
}

impl GraphNode for TestNode {
    fn visit(&self, _visitor: &mut GraphVisitor) {}
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct TestNodeSettings;

impl NodeSettings for TestNodeSettings {
    fn name() -> &'static str {
        "test"
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

impl NodeCompiler for TestNode {
    type Settings = TestNodeSettings;

    fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
        context.serialize_node(TestNode)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedTransitionNode {
    pub steps: RangeInclusive<u64>,
    pub progress: NumberMut<Alpha>,
}

impl GraphNodeConstructor for FixedTransitionNode {
    fn identity() -> &'static str {
        "fixed_transition"
    }

    fn construct_entry(self, _metrics: &GraphMetrics) -> anyhow::Result<GraphNodeEntry> {
        Ok(GraphNodeEntry::Process(Box::new(self.clone())))
    }
}

impl GraphNode for FixedTransitionNode {
    fn visit(&self, visitor: &mut GraphVisitor) {
        if self.steps.contains(&visitor.graph.iteration()) {
            self.progress.set(visitor.graph, Alpha(0.5));
        } else {
            self.progress.set(visitor.graph, ALPHA_ONE);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedTransitionSettings {
    pub steps: RangeInclusive<u64>,
}

impl Default for FixedTransitionSettings {
    fn default() -> Self {
        Self { steps: 0..=0 }
    }
}

impl NodeSettings for FixedTransitionSettings {
    fn name() -> &'static str {
        FixedTransitionNode::identity()
    }

    fn input() -> &'static [(&'static str, IOType)] {
        &[]
    }

    fn output() -> &'static [(&'static str, IOType)] {
        &[(DEFAULT_OUPTUT_NAME, IOType::Number)]
    }

    fn build(self) -> anyhow::Result<Extras> {
        Ok(serde_json::to_value(self)?)
    }
}

impl NodeCompiler for FixedTransitionNode {
    type Settings = FixedTransitionSettings;

    fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError> {
        let progress = context.output_number(0)?;
        let settings = context.settings::<FixedTransitionSettings>()?;
        context.serialize_node(FixedTransitionNode {
            steps: settings.steps,
            progress,
        })
    }
}

fn fixed_transition(target: &str, steps: RangeInclusive<usize>) -> Transition {
    Transition {
        target: target.into(),
        condition: graph_iteration()
            .ge(*steps.start() as f32)
            .and(graph_iteration().le(*steps.end() as f32)),
        forceable: false,
        node: Some(
            Node::new(
                [],
                [],
                FixedTransitionSettings {
                    steps: *steps.start() as u64..=*steps.end() as u64,
                },
            )
            .unwrap(),
        ),
        ..Default::default()
    }
    .with_progress(bind_route("node"))
    .with_duration(Seconds(1.0))
}

fn on_steps_for(target: &str, steps: RangeInclusive<usize>, duration: f32) -> Transition {
    Transition {
        target: target.into(),
        condition: graph_iteration()
            .ge(*steps.start() as f32)
            .and(graph_iteration().le(*steps.end() as f32)),
        forceable: false,
        ..Default::default()
    }
    .with_duration(Seconds(duration))
}

fn immediate_on_step(target: &str, step: usize) -> Transition {
    Transition {
        target: target.into(),
        condition: graph_iteration().compare_number(step as f64),
        forceable: false,
        immediate: true,
        ..Default::default()
    }
}

fn compile_machines(
    state_machines: impl Into<Vec<StateMachine>>,
) -> Box<GraphDefinitionCompilation> {
    let mut graph = AnimGraph {
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let mut registry = NodeCompilationRegistry::default();
    registry.register::<TestNode>();
    registry.register::<FixedTransitionNode>();

    let result = GraphDefinitionCompilation::compile(&graph, &registry).unwrap();

    // Editor route testing
    resolve_routes(&mut graph).unwrap();
    serialize_routes(&mut graph);
    GraphDefinitionCompilation::compile(&graph, &registry).unwrap();

    result
}

fn test_endpoint(name: &'static str) -> Branch {
    Branch {
        node: Some(
            Node::new_disconnected(TestNodeSettings)
                .expect("Valid")
                .with_alias(name),
        ),
        ..Default::default()
    }
}

#[test]
fn test_one_node() {
    let machines = compile_machines([state_machine(
        "Root",
        [state("Initial").with_branch(test_endpoint("A"))],
    )]);
    let mut ctx = TestMachineEvalutor::new(machines);
    ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
}

#[test]
fn test_transition() {
    #[rustfmt::skip]
        let machines = compile_machines(
            [state_machine("Root",
                [
                    state("StateA").with(test_endpoint("A"), [fixed_transition("StateB", 2..=2)]),
                    state("StateB").with(test_endpoint("B"), [immediate_on_step("StateC", 4)]),
                    state("StateC").with(test_endpoint("C"), []),
                ],
            )],
        );

    let mut ctx = TestMachineEvalutor::new(machines);
    ctx.expect_endpoints(&[
        &[("A", ALPHA_ONE)],
        &[("A", Alpha(0.5)), ("B", Alpha(0.5))],
        &[("A", ALPHA_ZERO), ("B", ALPHA_ONE)],
        &[("C", ALPHA_ONE)],
    ]);
}

#[test]
fn test_transition_duration() {
    #[rustfmt::skip]
        let machines = compile_machines(
            [state_machine("Root",
                [
                    state("StateA").with(test_endpoint("A"), [on_steps_for("StateB", 2..=2, 4.0)]),
                    state("StateB").with(test_endpoint("B"), []),
                ],
            )],
        );

    let mut ctx = TestMachineEvalutor::new(machines);
    ctx.expect_endpoints(&[
        &[("A", ALPHA_ONE)],
        &[("A", ALPHA_ONE), ("B", Alpha(0.0))],
        &[("A", Alpha(0.75)), ("B", Alpha(0.25))],
        &[("A", Alpha(0.5)), ("B", Alpha(0.5))],
        &[("A", Alpha(0.25)), ("B", Alpha(0.75))],
        &[("A", ALPHA_ZERO), ("B", ALPHA_ONE)],
        &[("B", ALPHA_ONE)],
    ]);
}

#[test]
fn test_submachine() {
    #[rustfmt::skip]
    let machines = compile_machines(
        [state_machine("Root",
            [
                state("StateToMachineA").with(submachine("SubMachineA"), [fixed_transition("StateC", 3..=3)]),
                state("StateC").with(test_endpoint("C"), [fixed_transition("StateToMachineA", 6..=6)]),
            ],
        ),
        state_machine("SubMachineA",
            [
                state("StateA").with(test_endpoint("A"), [fixed_transition("StateB", 2..=3)]),
                state("StateB").with(test_endpoint("B"), []),
            ],
        )],

    );

    let mut ctx = TestMachineEvalutor::new(machines);
    ctx.expect_endpoints(&[
        &[("A", ALPHA_ONE)],
        &[("A", Alpha(0.5)), ("B", Alpha(0.5))],
        &[("A", Alpha(0.25)), ("B", Alpha(0.25)), ("C", Alpha(0.5))],
        &[("A", ALPHA_ZERO), ("B", ALPHA_ZERO), ("C", ALPHA_ONE)],
        &[("C", ALPHA_ONE)],
        &[("C", Alpha(0.5)), ("A", Alpha(0.5))],
        &[("C", ALPHA_ZERO), ("A", ALPHA_ONE)],
        &[("A", ALPHA_ONE)],
    ]);
}

#[test]
#[rustfmt::skip]
fn test_simple_layers() {
    let machines = compile_machines(
        [state_machine("Root",
            [
                state("StateA").with_layers([test_endpoint("A"), test_endpoint("B")]).with_transitions([fixed_transition("StateB", 2..=2)]),
                state("StateB").with(test_endpoint("C"), []),
            ],
        )],
    );

    let mut ctx = TestMachineEvalutor::new(machines);

    ctx.expect_endpoints(&[
        &[("A", ALPHA_ZERO), ("B", ALPHA_ONE)],
        &[("A", ALPHA_ZERO), ("B", Alpha(0.5)), ("C", Alpha(0.5))],
        &[("A", ALPHA_ZERO), ("B", ALPHA_ZERO), ("C", ALPHA_ONE)],
        &[("C", ALPHA_ONE)],
    ]);
}

fn test_unary_condition(
    condition: impl Into<Expression>,
    initial: bool,
    params: &[(&str, &dyn std::any::Any, bool)],
) {
    #[rustfmt::skip]
        let machines = compile_machines(
            [state_machine("Root",
                [
                    state("StateA").with(test_endpoint("A"), []),
                    state("StateB").with(test_endpoint("B"), []).with_global_condition(condition),
                ],
            )],
        );

    let mut ctx = TestMachineEvalutor::new(machines);
    if initial {
        ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
    } else {
        ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
    }

    for count in 1..=params.len() {
        let mut expected = false;
        for &(name, var, res) in params.iter().take(count) {
            if !name.is_empty() {
                assert!(ctx.set(name, var));
            }
            expected = res;
        }

        if expected {
            ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
        } else {
            ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
        }
    }
}

#[test]
fn test_global_transition() {
    test_unary_condition(true, true, &[("", &(), true)]);
    test_unary_condition(
        bind_parameter::<bool>("p1").not(),
        true,
        &[("p1", &false, true)],
    );
    test_unary_condition(
        bind_parameter::<bool>("p1").not(),
        true,
        &[("p1", &true, false)],
    );
    test_unary_condition(bind_parameter::<bool>("p1"), false, &[("p1", &true, true)]);
    test_unary_condition(
        bind_parameter::<bool>("p1"),
        false,
        &[("p1", &false, false)],
    );
}

#[test]
fn test_global_transition_binary_expressions() {
    fn test_binary_expression(
        condition: Expression,
        initial: bool,
        eval: impl Fn(bool, bool) -> bool,
    ) {
        #[rustfmt::skip]
            let machines = compile_machines(
                [state_machine("Root",
                    [
                        state("StateA").with(test_endpoint("A"), []),
                        state("StateB").with(test_endpoint("B"), []).with_global_condition(condition),
                    ],
                )],
            );

        let mut ctx = TestMachineEvalutor::new(machines);
        if initial {
            ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
        } else {
            ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
        }
        for p1 in [true, false] {
            for p2 in [true, false] {
                assert!(ctx.set("p1", &p1));
                assert!(ctx.set("p2", &p2));
                if eval(p1, p2) {
                    ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
                } else {
                    ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
                }
            }
        }
    }

    test_binary_expression(
        bind_parameter::<bool>("p1").and(bind_parameter::<bool>("p2")),
        false,
        |a, b| a && b,
    );
    test_binary_expression(
        bind_parameter::<bool>("p1").or(bind_parameter::<bool>("p2")),
        false,
        |a, b| a || b,
    );
    test_binary_expression(
        bind_parameter::<bool>("p1").xor(bind_parameter::<bool>("p2")),
        false,
        |a, b| a ^ b,
    );
    test_binary_expression(
        bind_parameter::<bool>("p1").equals(bind_parameter::<bool>("p2")),
        true,
        |a, b| a == b,
    );
    test_binary_expression(
        bind_parameter::<bool>("p1").not_equal(bind_parameter::<bool>("p2")),
        false,
        |a, b| a != b,
    );
}

#[test]
fn test_global_transition_order() {
    fn test_order(condition: Expression, initial: bool, eval: impl Fn(f64, f64) -> bool) {
        #[rustfmt::skip]
            let machines = compile_machines(
                [state_machine("Root",
                    [
                        state("StateA").with(test_endpoint("A"), []),
                        state("StateB").with(test_endpoint("B"), []).with_global_condition(condition),
                    ],
                )],
            );

        let mut ctx = TestMachineEvalutor::new(machines);
        if initial {
            ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
        } else {
            ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
        }

        for p1 in [1.0, 2.0] {
            for p2 in [1.0, 2.0] {
                assert!(ctx.set("p1", &p1));
                assert!(ctx.set("p2", &p2));
                let res = eval(p1, p2);
                if res {
                    ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
                } else {
                    ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
                }
            }
        }
    }

    test_order(
        bind_parameter::<f32>("p1").lt(bind_parameter::<f32>("p2")),
        false,
        |a, b| a < b,
    );
    test_order(
        bind_parameter::<f32>("p1").gt(bind_parameter::<f32>("p2")),
        false,
        |a, b| a > b,
    );
    test_order(
        bind_parameter::<f32>("p1").ge(bind_parameter::<f32>("p2")),
        true,
        |a, b| a >= b,
    );
    test_order(
        bind_parameter::<f32>("p1").le(bind_parameter::<f32>("p2")),
        true,
        |a, b| a <= b,
    );
    test_order(
        bind_parameter::<f32>("p1").equals(bind_parameter::<f32>("p2")),
        true,
        |a, b| a == b,
    );
    test_order(
        bind_parameter::<f32>("p1").not_equal(bind_parameter::<f32>("p2")),
        false,
        |a, b| a != b,
    );
}

#[test]
fn test_global_transition_expression() {
    fn test_expr(condition: Expression, initial: bool, eval: impl Fn(f64, f64) -> bool) {
        #[rustfmt::skip]
            let machines = compile_machines(
                [state_machine("Root",
                    [
                        state("StateA").with(test_endpoint("A"), []),
                        state("StateB").with(test_endpoint("B"), []).with_global_condition(condition),
                    ],
                )],
            );

        let mut ctx = TestMachineEvalutor::new(machines);
        if initial {
            ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
        } else {
            ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
        }

        for p1 in [1.0, 2.0, -1.0, -2.0] {
            for p2 in [1.0, 2.0, -1.0, -2.0] {
                assert!(ctx.set("p1", &p1));
                assert!(ctx.set("p2", &p2));
                let res = eval(p1, p2);
                if res {
                    ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);
                } else {
                    ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);
                }
            }
        }
    }

    test_expr(
        bind_parameter::<f32>("p1")
            .add(bind_parameter::<f32>("p2"))
            .gt(0.0f64),
        false,
        |a, b| a + b > 0.0,
    );
    test_expr(
        bind_parameter::<f32>("p1")
            .subtract(bind_parameter::<f32>("p2"))
            .gt(0.0f64),
        false,
        |a, b| a - b > 0.0,
    );
    test_expr(
        bind_parameter::<f32>("p1")
            .multiply(bind_parameter::<f32>("p2"))
            .gt(0.0f64),
        false,
        |a, b| a * b > 0.0,
    );
    test_expr(
        bind_parameter::<f32>("p1")
            .divide(bind_parameter::<f32>("p2"))
            .gt(0.0f64),
        false,
        |a, b| a / b > 0.0,
    );
    test_expr(
        bind_parameter::<f32>("p1")
            .modulus(bind_parameter::<f32>("p2"))
            .gt(0.0f64),
        false,
        |a, b| a % b > 0.0,
    );
}

#[test]
fn test_event_condition() {
    let condition = event_is("EVENT_A", QueryType::Entered);

    #[rustfmt::skip]
        let machines = compile_machines(
            [state_machine("Root",
                [
                    state("StateA").with(test_endpoint("A"), []),
                    state("StateB").with(test_endpoint("B"), []).with_global_condition(condition),
                ],
            )],
        );

    let mut ctx = TestMachineEvalutor::new(machines);

    let event = ctx.data.event_by_name("EVENT_A").expect("Valid event");
    ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);

    for (state, expected) in [
        (FlowState::Exited, "A"),
        (FlowState::Entering, "A"),
        (FlowState::Exiting, "A"),
        (FlowState::Entered, "B"),
    ] {
        let mut graph = ctx.build_graph().unwrap();
        event.set(&mut graph, state);
        ctx.check_flow_endpoints(&[&[(expected, ALPHA_ONE)]], &mut graph);
    }
}

#[test]
fn test_event_active_condition() {
    let condition = event_is("EVENT_A", QueryType::Active);

    #[rustfmt::skip]
        let machines = compile_machines(
            [state_machine("Root",
                [
                    state("StateA").with(test_endpoint("A"), []),
                    state("StateB").with(test_endpoint("B"), []).with_global_condition(condition),
                ],
            )],
        );

    let mut ctx = TestMachineEvalutor::new(machines);

    let event = ctx.data.event_by_name("EVENT_A").expect("Valid event");
    ctx.expect_endpoints(&[&[("A", ALPHA_ONE)]]);

    for (state, expected) in [
        (FlowState::Exited, "A"),
        (FlowState::Entering, "B"),
        (FlowState::Exiting, "B"),
        (FlowState::Entered, "B"),
    ] {
        let mut graph = ctx.build_graph().unwrap();
        event.set(&mut graph, state);
        ctx.check_flow_endpoints(&[&[(expected, ALPHA_ONE)]], &mut graph);
    }
}

#[test]
fn test_state_condition() {
    let condition1 = state_is("Root::StateA", QueryType::Entered);
    let condition2 = state_is("Root::StateB", QueryType::Active);

    #[rustfmt::skip]
    let machines = compile_machines(
        [state_machine("Root",
            [
                state("StateA").with(submachine("Submachine1"), []),
                state("StateB")
                    .with(submachine("Submachine2"), [])
                    .with_global_condition(bind_parameter::<bool>("condition")),
            ],
        ),
        state_machine("Submachine1",
            [
                state("StateA").with(test_endpoint("A"), []),
                state("StateB").with(test_endpoint("B"), []).with_global_condition(condition1),
            ],
        ),
        state_machine("Submachine2",
            [
                state("StateA").with(test_endpoint("C"), []),
                state("StateB").with(test_endpoint("D"), []).with_global_condition(condition2),
            ],
        ),

        ],
    );

    let mut ctx = TestMachineEvalutor::new(machines);

    ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);

    ctx.set("condition", &true);
    ctx.expect_endpoints(&[&[("D", ALPHA_ONE)]]);
}

#[test]
fn test_machine_condition() {
    let condition1 = state_machine_is("Root", QueryType::Entered);
    let condition2 = state_machine_is("Root", QueryType::Active);

    #[rustfmt::skip]
    let machines = compile_machines(
        [state_machine("Root",
            [
                state("StateA").with(submachine("Submachine1"), []),
                state("StateB").with(submachine("Submachine2"), []).with_global_condition(bind_parameter::<bool>("condition")),
            ],
        ),
        state_machine("Submachine1",
            [
                state("StateA").with(test_endpoint("A"), []),
                state("StateB").with(test_endpoint("B"), []).with_global_condition(condition1),
            ],
        ),
        state_machine("Submachine2",
            [
                state("StateA").with(test_endpoint("C"), []),
                state("StateB").with(test_endpoint("D"), []).with_global_condition(condition2),
            ],
        ),

        ],
    );

    let mut ctx = TestMachineEvalutor::new(machines);

    ctx.expect_endpoints(&[&[("B", ALPHA_ONE)]]);

    ctx.set("condition", &true);
    ctx.expect_endpoints(&[&[("D", ALPHA_ONE)]]);
}

#[test]
fn test_contains_inclusive() {
    use crate::compiler::prelude::*;
    let state_machines = [state_machine(
        "Root",
        [
            state("Base").with_transitions([contains_inclusive(
                (1.0, 1.5),
                bind_parameter::<[f32; 3]>("point_of_interest").projection(Projection::Length),
            )
            .immediate_transition("Target")]),
            state("Target").with_branch(endpoint(reference_pose())),
        ],
    )];

    let anim_graph = AnimGraph {
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let compiled =
        GraphDefinitionCompilation::compile(&anim_graph, &default_compilation_nodes()).unwrap();
    let definition = compiled
        .builder
        .build(&default_node_constructors())
        .unwrap();
    let vec = definition
        .get_vector_parameter("point_of_interest")
        .unwrap();

    let mut graph = definition.build_with_empty_skeleton(Arc::new(EmptyResourceProvider));
    let mut context = DefaultRunContext::new(1.0);

    let range = 1.0..=1.5;
    for test in [0f32, 0.99, 1.0, 1.3, 1.5, 2.0] {
        graph.reset_graph();
        context.run(&mut graph);
        vec.set(&mut graph, [0.0, test, 0.0]);
        context.clear();
        context.run_without_blend(&mut graph);
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(
            context
                .tree
                .append(&graph, &context.layers)
                .unwrap()
                .is_some(),
            range.contains(&test),
            "Unexpected result for {test}"
        );
    }
}

#[test]
fn test_contains_exclusive() {
    use crate::compiler::prelude::*;
    let state_machines = [state_machine(
        "Root",
        [
            state("Base").with_transitions([contains_exclusive(
                (1.0, 1.5),
                bind_parameter::<[f32; 3]>("point_of_interest").projection(Projection::Length),
            )
            .immediate_transition("Target")]),
            state("Target").with_branch(endpoint(reference_pose())),
        ],
    )];

    let anim_graph = AnimGraph {
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let compiled =
        GraphDefinitionCompilation::compile(&anim_graph, &default_compilation_nodes()).unwrap();
    let definition = compiled
        .builder
        .build(&default_node_constructors())
        .unwrap();
    let vec = definition
        .get_vector_parameter("point_of_interest")
        .unwrap();

    let mut graph = definition.build_with_empty_skeleton(Arc::new(EmptyResourceProvider));
    let mut context = DefaultRunContext::new(1.0);

    let range = 1.0..1.5;
    for test in [0f32, 0.99, 1.0, 1.3, 1.5, 2.0] {
        graph.reset_graph();
        context.run(&mut graph);
        vec.set(&mut graph, [0.0, test, 0.0]);
        context.clear();
        context.run_without_blend(&mut graph);
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(
            context
                .tree
                .append(&graph, &context.layers)
                .unwrap()
                .is_some(),
            range.contains(&test),
            "Unexpected result for {test}"
        );
    }
}

#[test]
fn test_contains_inclusive_not() {
    use crate::compiler::prelude::*;
    let state_machines = [state_machine(
        "Root",
        [
            state("Base").with_transitions([contains_inclusive(
                (1.0, 1.5),
                bind_parameter::<[f32; 3]>("point_of_interest").projection(Projection::Length),
            )
            .not()
            .immediate_transition("Target")]),
            state("Target").with_branch(endpoint(reference_pose())),
        ],
    )];

    let anim_graph = AnimGraph {
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let compiled =
        GraphDefinitionCompilation::compile(&anim_graph, &default_compilation_nodes()).unwrap();
    let definition = compiled
        .builder
        .build(&default_node_constructors())
        .unwrap();
    let vec = definition
        .get_vector_parameter("point_of_interest")
        .unwrap();

    let mut graph = definition.build_with_empty_skeleton(Arc::new(EmptyResourceProvider));
    let mut context = DefaultRunContext::new(1.0);

    let range = 1.0..=1.5;
    for test in [0f32, 0.99, 1.0, 1.3, 1.5, 2.0] {
        graph.reset_graph();
        context.run(&mut graph);
        vec.set(&mut graph, [0.0, test, 0.0]);
        context.clear();
        context.run_without_blend(&mut graph);
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(
            context
                .tree
                .append(&graph, &context.layers)
                .unwrap()
                .is_some(),
            !range.contains(&test),
            "Unexpected result for {test}"
        );
    }
}

#[test]
fn test_contains_exclusive_not() {
    use crate::compiler::prelude::*;
    let state_machines = [state_machine(
        "Root",
        [
            state("Base").with_transitions([contains_exclusive(
                (1.0, 1.5),
                bind_parameter::<[f32; 3]>("point_of_interest").projection(Projection::Length),
            )
            .not()
            .immediate_transition("Target")]),
            state("Target").with_branch(endpoint(reference_pose())),
        ],
    )];

    let anim_graph = AnimGraph {
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let compiled =
        GraphDefinitionCompilation::compile(&anim_graph, &default_compilation_nodes()).unwrap();
    let definition = compiled
        .builder
        .build(&default_node_constructors())
        .unwrap();
    let vec = definition
        .get_vector_parameter("point_of_interest")
        .unwrap();

    let mut graph = definition.build_with_empty_skeleton(Arc::new(EmptyResourceProvider));
    let mut context = DefaultRunContext::new(1.0);

    let range = 1.0..1.5;
    for test in [0f32, 0.99, 1.0, 1.3, 1.5, 2.0] {
        graph.reset_graph();
        context.run(&mut graph);
        vec.set(&mut graph, [0.0, test, 0.0]);
        context.clear();
        context.run_without_blend(&mut graph);
        context
            .tree
            .set_reference_task(Some(BlendSampleId::Task(123)));
        assert_eq!(
            context
                .tree
                .append(&graph, &context.layers)
                .unwrap()
                .is_some(),
            !range.contains(&test),
            "Unexpected result for {test}"
        );
    }
}

#[test]
fn test_recursive_branch() {
    let mut machines = compile_machines([state_machine(
        "Root",
        [state("Initial").with(test_endpoint("A"), [])],
    )]);

    {
        machines.builder.desc.branches[0] = HierarchicalBranch {
            node: None,
            target: HierarchicalBranchTarget::Machine(MachineIndex(0)),
        };
    }

    assert!(serialize_and_compile(machines.builder).is_err());
}

#[test]
fn test_recursive_layers() {
    let mut machines = compile_machines([state_machine(
        "Root",
        [state("Initial").with(test_endpoint("A"), [])],
    )]);

    {
        machines.builder.desc.branches[0] = HierarchicalBranch {
            node: None,
            target: HierarchicalBranchTarget::Layers { start: 0, count: 1 },
        };
    }

    assert!(serialize_and_compile(machines.builder).is_err());
}

#[test]
fn test_infinite_recursion_condition() {
    #[rustfmt::skip]
    let mut machines = compile_machines(
        [state_machine("Root",
            [
                state("StateA").with(test_endpoint("A"), []).with_global_condition(bind_parameter::<bool>("param1").and(bind_parameter::<bool>("RECURSION"))),
            ],
        )],
    );

    assert_eq!(machines.builder.desc.total_subconditions(), 2);
    {
        machines.builder.desc.conditions[0] = GraphCondition::all_of(0..1);
    }

    assert!(serialize_and_compile(machines.builder).is_err());
}

#[test]
fn test_recursive_machine() {
    let state_machines = [state_machine(
        "Root",
        [
            state("StateA").with(test_endpoint("A"), [fixed_transition("Recursive", 2..=2)]),
            state("Recursive").with(submachine("Root"), []),
        ],
    )];

    let graph = AnimGraph {
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let mut registry = NodeCompilationRegistry::default();
    registry.register::<TestNode>();

    assert!(GraphDefinitionCompilation::compile(&graph, &registry).is_err());
}

#[test]
fn test_empty_branches() {
    let mut machines = compile_machines([state_machine("Root", [state("A")])]);
    machines.builder.desc.machine_states[0] = 0;
    assert!(serialize_and_compile(machines.builder).is_err());
}
