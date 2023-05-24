use std::ops::RangeInclusive;
use std::sync::Arc;

use crate::compiler::{resolve_routes, serialize_routes, GraphDefinitionCompilation};
use crate::processors::FlowEvents;
use crate::AnimationClip;
use crate::{
    compiler::prelude::*, core::*, AnimationId, BlendSample, BlendSampleId, BlendTree,
    FlowEventsHook, Graph, Interpreter, InterpreterContext, LayerBuilder,
};

fn pose_interpolate(a: u16, b: u16, t: f32) -> BlendSample {
    // NOTE: tests were written with 1-based indices
    assert!(a > 0);
    assert!(b > 0);
    BlendSample::Interpolate(BlendSampleId::Task(a - 1), BlendSampleId::Task(b - 1), t)
}

fn pose_blend(a: u16, b: u16, t: f32) -> BlendSample {
    // NOTE: tests were written with 1-based indices
    assert!(a > 0);
    assert!(b > 0);
    BlendSample::Blend(
        BlendSampleId::Task(a - 1),
        BlendSampleId::Task(b - 1),
        t,
        BoneGroupId::All,
    )
}

fn clip_asset(id: u32, start: Seconds, duration: Seconds) -> AnimationClip {
    AnimationClip {
        animation: AnimationId(id),
        bone_group: BoneGroupId::All,
        start,
        duration,
        looping: true,
    }
}

fn clip_sample(clip: &AnimationClip, dt: f32) -> BlendSample {
    let mut timer = clip.init_timer();
    timer.tick(Seconds(dt));
    BlendSample::Animation {
        id: clip.animation,
        normalized_time: timer.time().0,
    }
}

fn resource_provider(definition: &GraphDefinition) -> Arc<dyn GraphResourceProvider> {
    let default_value = AnimationClip {
        animation: AnimationId(u32::MAX),
        bone_group: BoneGroupId::All,
        looping: false,
        start: Default::default(),
        duration: Default::default(),
    };

    Arc::new(
        SimpleResourceProvider::new_with_map(
            definition,
            default_value,
            |_resource_type, content| {
                let clip: AnimationClip = serde_json::from_value(content)?;
                Ok(clip)
            },
        )
        .unwrap(),
    )
}

fn constructor_provider() -> GraphNodeRegistry {
    let mut registry = GraphNodeRegistry::default();
    add_default_constructors(&mut registry);
    registry
}

fn run_animation_steps<'a>(
    machines: Box<GraphDefinitionCompilation>,
    mut cb: impl FnMut(&mut Graph) -> f64,
    expected: impl IntoIterator<Item = &'a [BlendSample]>,
) {
    run_animation_steps_events(machines, |graph, _events| cb(graph), expected);
}

fn run_animation_steps_events<'a>(
    machines: Box<GraphDefinitionCompilation>,
    mut cb: impl FnMut(&mut Graph, &FlowEvents) -> f64,
    expected: impl IntoIterator<Item = &'a [BlendSample]>,
) {
    let serialized = serde_json::to_value(machines.builder.clone()).unwrap();

    let builder: GraphDefinitionBuilder = serde_json::from_value(serialized).unwrap();
    let definition = builder.build(&constructor_provider()).unwrap();
    let resources = resource_provider(&definition);
    let mut graph = definition.build_with_empty_skeleton(resources);

    struct GraphHook(Box<GraphDefinitionCompilation>);
    impl FlowEventsHook for GraphHook {
        fn debug_trigger(
            &mut self,
            _graph: &Graph,
            _context: &InterpreterContext,
            debug_break: crate::graph::GraphDebugBreak,
            result: bool,
        ) {
            if let Some(var) = self.0.get_debug_trigger(&debug_break) {
                println!("{}: {}", var, result);
            }
        }
    }

    graph.set_events_hook(Box::new(GraphHook(machines)));

    let mut layers = LayerBuilder::default();
    let mut events = FlowEvents::default();

    let definition = graph.definition().clone();

    for samples in expected.into_iter() {
        let step = graph.iteration() + 1;
        let dt = cb(&mut graph, &events);
        layers.clear();
        events.clear();
        let runner = Interpreter::run(&mut graph, &definition, &mut events, &mut layers, dt);

        let mut tasks = BlendTree::default();

        let graph = &mut *runner.visitor.graph;
        let layers = &mut *runner.layers;

        let _result = tasks.append(graph, layers).unwrap();

        assert_eq!(
            tasks.get(),
            samples,
            "unexpected tasks at #{step}: {:#?}",
            tasks.get(),
        );
    }
}

pub fn init<T: Into<InitialParameterValue>>(p: (&str, T)) -> Parameter {
    Parameter::new(p.0.to_owned(), p.1.into())
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
    parameters: impl Into<Vec<Parameter>>,
    resources: impl Into<Vec<ResourceContent>>,
    state_machines: impl Into<Vec<StateMachine>>,
) -> Box<GraphDefinitionCompilation> {
    let mut graph = AnimGraph {
        parameters: parameters.into(),
        resources: resources.into(),
        state_machines: state_machines.into(),
        ..Default::default()
    };

    let mut registry = NodeCompilationRegistry::default();
    add_default_nodes(&mut registry);

    let result = GraphDefinitionCompilation::compile(&graph, &registry).unwrap();

    // Editor route testing
    resolve_routes(&mut graph).unwrap();
    serialize_routes(&mut graph);
    GraphDefinitionCompilation::compile(&graph, &registry).unwrap();

    result
}

fn empty_endpoint() -> Branch {
    Branch::default()
}
fn on_parameter(name: &str) -> Expression {
    Expression::Parameter(name.into())
}

#[test]
fn test_empty() {
    let jumping = clip_asset(0, Seconds(0.5), Seconds(1.0));
    let resources = [(jumping.build_content("jumping").unwrap())];

    let jumping_endpoint = endpoint(alias("Jumping", animation_pose("jumping")));
    let machines = compile_machines(
        [init(("Jump", false))],
        resources,
        [state_machine(
            "Action",
            [
                state("Off").with(
                    empty_endpoint(),
                    [on_parameter("Jump").transition("Jump", Seconds(4.0))],
                ),
                state("Jump").with(jumping_endpoint, []),
            ],
        )],
    );

    let jump = machines.get_boolean_parameter("Jump").unwrap();

    let expected: [&[BlendSample]; 2] = [&[], &[clip_sample(&jumping, 0.0)]];
    run_animation_steps(
        machines,
        |graph| {
            match graph.iteration() {
                1 => {
                    jump.set(graph, true);
                }
                _ => {}
            }

            1.0
        },
        expected,
    );
}

#[test]
fn test_endpoint_submachine() {
    let a0 = clip_asset(0, Seconds(0.5), Seconds(4.0));
    let a1 = clip_asset(1, Seconds(0.5), Seconds(2.0));
    let a2 = clip_asset(2, Seconds(0.3), Seconds(3.0));

    let resources = [
        a0.build_content("a0").unwrap(),
        a1.build_content("a1").unwrap(),
        a2.build_content("a2").unwrap(),
    ];

    let e0 = endpoint(alias("A", animation_pose("a0")));
    let e1 = endpoint(alias("B", animation_pose("a1")));
    let e2 = endpoint(alias("C", animation_pose("a2")));

    #[rustfmt::skip]
    let machines = compile_machines(
        [], resources,
        [
        state_machine("Root",
            [
                state("StateA").with(e0, [on_steps_for("StateB", 2..=2, 4.0)]),
                state("StateB").with(submachine("Submachine"), []),
            ],
        ),
        state_machine("Submachine",
            [
                state("StateC").with(e1, [on_steps_for("StateD", 3..=3, 4.0)]),
                state("StateD").with(e2, []),
            ],
        )
    ],);

    #[rustfmt::skip]
    let expected: [&[BlendSample]; 8] = [
        &[clip_sample(&a0, 0.0)],
        &[clip_sample(&a0, 1.0)],
        &[clip_sample(&a0, 2.0), clip_sample(&a1, 1.0), pose_interpolate(1, 2, 0.25)],
        &[clip_sample(&a0, 3.0), clip_sample(&a1, 2.0), clip_sample(&a2, 1.0), pose_interpolate(2, 3, 0.25), pose_interpolate(1, 4, 0.5)],
        &[clip_sample(&a0, 4.0), clip_sample(&a1, 3.0), clip_sample(&a2, 2.0), pose_interpolate(2, 3, 0.5), pose_interpolate(1, 4, 0.75)],
        &[clip_sample(&a0, 5.0), clip_sample(&a1, 4.0), clip_sample(&a2, 3.0), pose_interpolate(2, 3, 0.75), pose_interpolate(1, 4, 1.0)],
        &[clip_sample(&a1, 5.0), clip_sample(&a2, 4.0), pose_interpolate(1, 2, 1.0)],
        &[clip_sample(&a2, 5.0)],
    ];

    #[rustfmt::skip]
    run_animation_steps(machines, |_| 1.0, expected);
}

fn inactive_layer_endpoint(name: &str) -> Branch {
    Branch {
        node: Some(inactive_layer().with_alias(name)),
        ..Default::default()
    }
}

#[test]
fn test_layer_weight() {
    let jumping = clip_asset(0, Seconds(0.5), Seconds(2.0));
    let idle = clip_asset(1, Seconds(0.5), Seconds(3.0));

    let resources = [
        jumping.build_content("jumping").unwrap(),
        idle.build_content("idle").unwrap(),
    ];

    let jumping_endpoint = endpoint(alias("Jumping", animation_pose("jumping")));
    let idle_endpoint = endpoint(alias("Idle", animation_pose("idle")));

    #[rustfmt::skip]
    let machines = compile_machines(
        [init(("Jump", false))],
            resources,
        [
        state_machine("Root",[
            state("LAYERS").with_layers([submachine("Locomotion"), submachine("Action")]),
        ],),
        state_machine("Locomotion", [
            state("IDLE").with(idle_endpoint, []),
        ],),
        state_machine("Action", [
            state("OFF").with(inactive_layer_endpoint("off"), [
                on_parameter("Jump").transition("JUMP", Seconds(4.0))
            ]),
            state("JUMP").with(jumping_endpoint, [
                on_parameter("Jump").not().transition("OFF", Seconds(4.0))
            ]),
        ],),
    ],);

    #[rustfmt::skip]
    let expected: [&[BlendSample]; 11] = [
        &[clip_sample(&idle, 0.0)],
        // Jump start
        &[clip_sample(&idle, 1.0)],
        &[clip_sample(&idle, 2.0), clip_sample(&jumping, 1.0), pose_blend(1, 2, 0.25),],
        &[clip_sample(&idle, 3.0), clip_sample(&jumping, 2.0), pose_blend(1, 2, 0.50),],
        &[clip_sample(&idle, 4.0), clip_sample(&jumping, 3.0), pose_blend(1, 2, 0.75),],
        &[clip_sample(&idle, 5.0), clip_sample(&jumping, 4.0), pose_blend(1, 2, 1.0),],
        // Idle start
        &[clip_sample(&idle, 6.0), clip_sample(&jumping, 5.0), pose_blend(1, 2, 1.0),],
        &[clip_sample(&idle, 7.0), clip_sample(&jumping, 6.0), pose_blend(1, 2, 0.75),],
        &[clip_sample(&idle, 8.0), clip_sample(&jumping, 7.0), pose_blend(1, 2, 0.50),],
        &[clip_sample(&idle, 9.0), clip_sample(&jumping, 8.0), pose_blend(1, 2, 0.25),],
        &[clip_sample(&idle, 10.0)],
    ];

    let jump = machines.get_boolean_parameter("Jump").unwrap();

    run_animation_steps(
        machines,
        |graph| {
            match graph.iteration() {
                1 => {
                    jump.set(graph, true);
                }
                6 => {
                    println!("Setting jump to false");
                    jump.set(graph, false);
                }
                _ => {}
            }

            1.0
        },
        expected,
    );
}

#[test]
fn test_animation_clip() {
    let a0 = clip_asset(0, Seconds(0.0), Seconds(4.0));
    let resources = [a0.build_content("anim_0").unwrap()];

    let anim_clip_node = endpoint(alias("anim_node", animation_pose("anim_0")));

    let speed_scaled = preprocess(
        tree([
            blend_in(ALPHA_ZERO, Seconds(4.0)),
            speed_scale(2.0, bind_route("blend_in")),
        ]),
        anim_clip_node,
    );

    #[rustfmt::skip]
    let machines = compile_machines(
        [],
            resources,
        [state_machine("Root",
            [
                state("A").with(speed_scaled, [immediate_on_step("B", 6)]),
                state("B").with(empty_endpoint(), [immediate_on_step("A", 7)])
            ],
        )],
    );

    /* eventually TODO, should be possible to once nodes can be instantiated by the definition
    let blend_alias = machines.node_number::<UNorm>("anim_node/blend_in");
    assert!(blend_alias.is_some());

    let blend_in = machines.node_number::<UNorm>("::Root::A::branch/node/blend_in");
    assert!(blend_in.is_some());
    assert_eq!(blend_alias, blend_in);
    */

    #[rustfmt::skip]
    let expected: [&[BlendSample]; 11] = [
        &[clip_sample(&a0, 0.0)],
        &[clip_sample(&a0, 1.25)],
        &[clip_sample(&a0, 2.75)],
        &[clip_sample(&a0, 4.5)],
        &[clip_sample(&a0, 6.5)],
        &[],
        &[clip_sample(&a0, 0.0)],
        &[clip_sample(&a0, 1.25)],
        &[clip_sample(&a0, 2.75)],
        &[clip_sample(&a0, 4.5)],
        &[clip_sample(&a0, 6.5)],
    ];

    run_animation_steps(machines, |_graph| 1.0, expected);
}

#[test]
fn test_third_person_character() {
    let idle_animation = clip_asset(0, Seconds(0.0), Seconds(4.0));
    let walking_animation = clip_asset(1, Seconds(0.0), Seconds(4.0));
    let running_animation = clip_asset(2, Seconds(0.0), Seconds(4.0));
    let jump_animation = clip_asset(3, Seconds(0.0), Seconds(4.0));
    let falling_animation = clip_asset(4, Seconds(0.0), Seconds(4.0));

    let resources = [
        idle_animation.build_content("idle").unwrap(),
        walking_animation.build_content("walking").unwrap(),
        running_animation.build_content("running").unwrap(),
        jump_animation.build_content("jumping").unwrap(),
        falling_animation.build_content("falling").unwrap(),
    ];

    let locomotion_layer = {
        let idle_node = animation_pose("idle");
        let walking_node = animation_pose("walking");
        let running_node = animation_pose("running");

        let locomotion_blend = endpoint(alias(
            "locomotion",
            blend_ranges(
                bind_parameter("locomotion_speed"),
                [(0.0, idle_node), (2.0, walking_node), (6.0, running_node)],
            ),
        ));

        state_machine(
            "LOCOMOTION_LAYER",
            [state("BLEND").with(locomotion_blend, [])],
        )
    };

    const TRANSITION_DURATION: Seconds = Seconds(2.0);

    const EVENT_JUMP: &'static str = "EVENT_JUMP";
    const EVENT_FALLING: &'static str = "EVENT_FALLING";
    const EVENT_LANDED: &'static str = "EVENT_LANDED";

    let jump_layer = {
        let off_to_falling = bind_parameter::<bool>("grounded")
            .not()
            .and(bind_parameter::<bool>("falling"))
            .transition("FALLING", TRANSITION_DURATION);
        let off_to_jumping = bind_parameter::<bool>("grounded")
            .not()
            .and(bind_parameter::<bool>("falling").not())
            .transition("JUMPING", TRANSITION_DURATION);

        let jumping_to_off = on_parameter("grounded").transition("OFF", TRANSITION_DURATION);
        let jumping_to_falling = on_parameter("falling").transition("FALLING", TRANSITION_DURATION);
        let falling_to_off = on_parameter("grounded").transition("OFF", TRANSITION_DURATION);

        let jumping_clip = endpoint(alias(
            "jumping_clip",
            tree([
                state_event(EVENT_JUMP, true, EventEmit::Entry),
                animation_pose("jumping"),
            ]),
        ));
        let falling_clip = endpoint(alias(
            "falling_clip",
            tree([
                state_event(EVENT_FALLING, true, EventEmit::Entry),
                animation_pose("falling"),
            ]),
        ));
        state_machine(
            "JUMP_LAYER",
            [
                state("OFF").with(
                    endpoint(alias(
                        "off",
                        tree([
                            state_event(
                                EVENT_LANDED,
                                event_is(EVENT_FALLING, QueryType::Exiting),
                                EventEmit::Entry,
                            ),
                            inactive_layer(),
                        ]),
                    )),
                    [off_to_falling, off_to_jumping],
                ),
                state("JUMPING").with(jumping_clip, [jumping_to_off, jumping_to_falling]),
                state("FALLING").with(falling_clip, [falling_to_off]),
            ],
        )
    };

    let root = state_machine(
        "ROOT",
        [state("LAYERS").with_layers([submachine("LOCOMOTION_LAYER"), submachine("JUMP_LAYER")])],
    );

    let machines = compile_machines(
        [
            init(("grounded", true)),
            init(("falling", true)),
            init(("locomotion_speed", 0.0f32)),
        ],
        resources,
        [root, locomotion_layer, jump_layer],
    );

    let grounded = machines.get_boolean_parameter("grounded").unwrap();
    let falling = machines.get_boolean_parameter("falling").unwrap();
    let locomotion_speed = machines.get_number_parameter("locomotion_speed").unwrap();

    #[rustfmt::skip]
    let expected: [&[BlendSample]; 18] = [
        &[clip_sample(&idle_animation, 0.0)],
        &[clip_sample(&idle_animation, 1.0)],
        &[clip_sample(&idle_animation, 2.0), clip_sample(&walking_animation, 2.0), pose_blend(1, 2, 0.5)],
        &[clip_sample(&walking_animation, 3.0)],
        &[clip_sample(&walking_animation, 4.0), clip_sample(&running_animation, 4.0), pose_blend(1, 2, 0.25)],
        &[clip_sample(&walking_animation, 5.0), clip_sample(&running_animation, 5.0), pose_blend(1, 2, 0.5)],
        &[clip_sample(&walking_animation, 6.0), clip_sample(&running_animation, 6.0), pose_blend(1, 2, 0.75)],
        &[clip_sample(&running_animation, 7.0)],
        // grounded = false, falling = false
        &[clip_sample(&running_animation, 8.0)],
        &[clip_sample(&running_animation, 9.0), clip_sample(&jump_animation, 1.0), pose_blend(1, 2, 0.5)],
        &[clip_sample(&running_animation, 10.0), clip_sample(&jump_animation, 2.0), pose_blend(1, 2, 1.0)],
        &[clip_sample(&running_animation, 11.0), clip_sample(&jump_animation, 3.0), pose_blend(1, 2, 1.0)],
        // falling = true
        &[clip_sample(&running_animation, 12.0), clip_sample(&jump_animation, 4.0), pose_blend(1, 2, 1.0)],
        &[clip_sample(&running_animation, 13.0), clip_sample(&jump_animation, 5.0), clip_sample(&falling_animation, 1.0), pose_interpolate(2, 3, 0.5), pose_blend(1, 4, 1.0)],
        // grounded = true
        &[clip_sample(&running_animation, 14.0), clip_sample(&jump_animation, 6.0), clip_sample(&falling_animation, 2.0), pose_interpolate(2, 3, 1.0), pose_blend(1, 4, 1.0)],
        &[clip_sample(&running_animation, 15.0), clip_sample(&falling_animation, 3.0), pose_blend(1, 2, 0.5)],
        &[clip_sample(&running_animation, 16.0)],
        &[clip_sample(&idle_animation, 17.0)],
    ];

    run_animation_steps_events(
        machines,
        |graph, events| {
            match graph.iteration() {
                x @ 2..=7 => {
                    let speed = (x - 1) as f32;
                    locomotion_speed.set(graph, speed);
                }
                8 => {
                    grounded.set(graph, false);
                    falling.set(graph, false);
                }

                12 => {
                    falling.set(graph, true);
                }
                14 => {
                    falling.set(graph, false);
                    grounded.set(graph, true);
                }
                17 => {
                    locomotion_speed.set(graph, 0.0);
                }
                _ => {}
            }

            const EVENT_ID_JUMP: Id = Id::from_str(EVENT_JUMP);
            const EVENT_ID_FALLING: Id = Id::from_str(EVENT_FALLING);
            const EVENT_ID_LANDED: Id = Id::from_str(EVENT_LANDED);

            match graph.iteration() {
                9 => assert_eq!(&events.emitted, &[EVENT_ID_JUMP]),
                13 => assert_eq!(&events.emitted, &[EVENT_ID_FALLING]),
                15 => assert_eq!(&events.emitted, &[EVENT_ID_LANDED]),
                _ => {
                    assert!(
                        events.emitted.is_empty(),
                        "events where emitted on: {}, {:?}",
                        graph.iteration(),
                        events
                            .emitted
                            .iter()
                            .map(|x| x == &EVENT_ID_JUMP)
                            .collect::<Vec<_>>(),
                    );
                }
            }
            1.0
        },
        expected,
    );
}

#[test]
fn test_idle_state_machine() {
    fn create_clip_asset(name: &str, url: &str) -> ResourceContent {
        ResourceContent {
            name: name.to_owned(),
            resource_type: AnimationClip::RESOURCE_TYPE.to_owned(),
            content: Value::String(url.to_owned()),
            ..Default::default()
        }
    }

    // 1. Create an instance of the data model
    let idle_state_machine: StateMachine = state_machine(
        "Root",
        [state("Idle").with_branch(endpoint(animation_pose("idle_looping")))],
    );

    let graph_model = AnimGraph {
        resources: [create_clip_asset("idle_looping", "character_idle.anim")].into(),
        state_machines: [idle_state_machine].into(),
        ..Default::default()
    };

    // 2. Compile definition with compilation nodes from the data model
    let definition_builder =
        match GraphDefinitionCompilation::compile(&graph_model, &default_compilation_nodes()) {
            Ok(compilation) => compilation.builder,
            Err(err) => panic!("Compilation error: {err:?}"),
        };

    // 3. Instantiate the definition with runtime nodes
    let definition = definition_builder
        .build(&default_node_constructors())
        .expect("A valid graph definition");

    // 4. Select resources for the runtime graph
    let resources = Arc::new(
        SimpleResourceProvider::new_with_map(
            &definition,
            AnimationClip::default(),
            |_resource_type, _content| Ok(AnimationClip::default()),
        )
        .expect("Valid definition resources"),
    );

    // 5. Create an instance of the runtime graph
    let mut graph = definition.build_with_empty_skeleton(resources);

    // 6. Evaluate the graph
    let delta_time = 0.123;
    let mut context = DefaultRunContext::new(delta_time);
    context.run(&mut graph);

    // 7. Update the blend tree for the target model instance
    let character_id = 123;
    set_animation_blend_tree(character_id, &context.tree);
    fn set_animation_blend_tree(_id: i32, _tree: &BlendTree) {}
}
