//! Example showing how to construct, serialize, compile, instantiate and run graphs

use animgraph::compiler::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

fn main() -> anyhow::Result<()> {
    let (locomotion_definition, default_locomotion_skeleton, default_locomotion_resources) =
        locomotion_graph_example()?;

    let mut locomotion = create_instance(
        locomotion_definition,
        default_locomotion_skeleton,
        default_locomotion_resources,
    );

    let (action_definition, default_action_skeleton, default_action_resources) =
        action_graph_example()?;

    let mut action = create_instance(
        action_definition,
        default_action_skeleton,
        default_action_resources,
    );

    // Parameter lookups can be done ahead of time using the definition which each graph has a reference to.
    let locomotion_speed = locomotion
        .definition()
        .get_number_parameter::<f32>("locomotion_speed")
        .expect("Valid parameter");
    let action_sit = action
        .definition()
        .get_event_parameter("sit")
        .expect("Valid parameter");

    // Parameters are specific to a definition
    locomotion_speed.set(&mut locomotion, 2.0);

    // Event has multiple states that micmics that of a state in a statemachine (entering, entered, exiting, exited)
    action_sit.set(&mut action, FlowState::Entered);

    let delta_time = 0.1;
    let mut context = DefaultRunContext::new(delta_time);
    let resources = RESOURCES.lock().expect("Single threaded");
    for frame in 1..=5 {
        // Multiple graphs can be concatenated where the output of the first is set
        // as a reference to the next. It's ultimately up to the next graph if it decides
        // to blend with the reference task at all.
        context.run(&mut action);

        // In this example the second graph looks for an emitted event from previous graphs
        // to decided if it should blend or not.
        context.run_and_append(&mut locomotion);

        // The resulting blend tree is all the active animations that could be sampled
        // even if they dont contribute to the final blend for things like animation events.
        // The tree can be evaluated from the last sample to form a trimmed blend stack.

        println!("Frame #{frame}:");
        println!("- Blend Tree:");
        for (index, task) in context.tree.get().iter().enumerate() {
            let n = index + 1;
            match task {
                BlendSample::Animation {
                    id,
                    normalized_time,
                } => {
                    println!(
                        "    #{n} Sample {} at t={normalized_time}",
                        &resources.animations[id.0 as usize].name
                    );
                }
                BlendSample::Blend(_, _, a, g) => {
                    if *g == BoneGroupId::All {
                        println!("    #{n} Blend a={a}");
                    } else {
                        println!("    #{n} Masked blend a={a}");
                    }
                }
                BlendSample::Interpolate(_, _, a) => {
                    println!("    #{n} Interpolate a={a}");
                }
            }
        }

        struct BlendStack<'a>(&'a GlobalResources);
        impl<'a> BlendTreeVisitor for BlendStack<'a> {
            fn visit(&mut self, tree: &BlendTree, sample: &BlendSample) {
                match *sample {
                    BlendSample::Animation {
                        id,
                        normalized_time,
                    } => {
                        println!(
                            "    Sample {} at t={normalized_time}",
                            &self.0.animations[id.0 as usize].name
                        );
                    }
                    BlendSample::Interpolate(x, y, a) => {
                        if a < 1.0 {
                            tree.visit(self, x);
                        }
                        if a > 0.0 {
                            tree.visit(self, y);
                        }

                        if a > 0.0 && a < 1.0 {
                            println!("    Interpolate a={a}");
                        }
                    }
                    BlendSample::Blend(x, y, a, g) => {
                        if g == BoneGroupId::All {
                            if a < 1.0 {
                                tree.visit(self, x);
                            }
                            if a > 0.0 {
                                tree.visit(self, y);
                            }

                            if a > 0.0 && a < 1.0 {
                                println!("    Interpolate a={a}");
                            }
                        } else {
                            tree.visit(self, x);
                            tree.visit(self, y);
                            println!("    Masked Blend a={a}");
                        }
                    }
                }
            }
        }

        println!("\n- Blend Stack:");
        context.tree.visit_root(&mut BlendStack(&resources));
        println!();
    }

    Ok(())
}

fn locomotion_graph_example() -> anyhow::Result<(
    Arc<GraphDefinition>,
    Option<Arc<Skeleton>>,
    Arc<dyn GraphResourceProvider>,
)> {
    // The constructed data model can be serialized and reused
    let locomotion_graph = create_locomotion_graph();
    let serialized_locmotion_graph = serde_json::to_string_pretty(&locomotion_graph)?;
    std::fs::write("locomotion.ag", serialized_locmotion_graph)?;

    // The specific nodes allowed is decided by the compilation registry
    let mut registry = NodeCompilationRegistry::default();
    add_default_nodes(&mut registry);

    // The resulting compilation contains additional debug information but only the builder is needed for the runtime
    let locomotion_compilation = GraphDefinitionCompilation::compile(&locomotion_graph, &registry)?;
    let serialize_locomotion_definition =
        serde_json::to_string_pretty(&locomotion_compilation.builder)?;
    std::fs::write("locomotion.agc", serialize_locomotion_definition)?;

    // The specific nodes instantiated at runtime is decided by the graph node registry
    let mut graph_nodes = GraphNodeRegistry::default();
    add_default_constructors(&mut graph_nodes);

    // The builder validates the definition and instantiates the immutable graph nodes which processes the graph data
    let locomotion_definition = locomotion_compilation.builder.build(&graph_nodes)?;

    // Resources are currently application defined. SimpleResourceProvider and the implementation in this example is illustrative of possible use.
    let default_locomotion_resources = Arc::new(SimpleResourceProvider::new_with_map(
        &locomotion_definition,
        RuntimeResource::Empty,
        get_cached_resource,
    )?);

    // Lookup default skeleton to use since there are no actual resources to probe
    let default_skeleton = default_locomotion_resources
        .resources
        .iter()
        .find_map(|x| match x {
            RuntimeResource::Skeleton(skeleton) => Some(skeleton.clone()),
            _ => None,
        });

    Ok((
        locomotion_definition,
        default_skeleton,
        default_locomotion_resources,
    ))
}

fn action_graph_example() -> anyhow::Result<(
    Arc<GraphDefinition>,
    Option<Arc<Skeleton>>,
    Arc<dyn GraphResourceProvider>,
)> {
    // The constructed data model can be serialized and reused
    let action_graph = create_action_graph();
    let serialized_locmotion_graph = serde_json::to_string_pretty(&action_graph)?;
    std::fs::write("action.ag", serialized_locmotion_graph)?;

    // The specific nodes allowed is decided by the compilation registry
    let mut registry = NodeCompilationRegistry::default();
    add_default_nodes(&mut registry);

    // The resulting compilation contains additional debug information but only the builder is needed for the runtime
    let action_compilation = GraphDefinitionCompilation::compile(&action_graph, &registry)?;
    let serialize_action_definition = serde_json::to_string_pretty(&action_compilation.builder)?;
    std::fs::write("action.agc", serialize_action_definition)?;

    // The specific nodes instantiated at runtime is decided by the graph node registry
    let mut graph_nodes = GraphNodeRegistry::default();
    add_default_constructors(&mut graph_nodes);

    // The builder validates the definition and instantiates the immutable graph nodes which processes the graph data
    let action_definition = action_compilation.builder.build(&graph_nodes)?;

    // Resources are currently application defined. SimpleResourceProvider and the implementation in this example is illustrative of possible use.
    let default_action_resources = Arc::new(SimpleResourceProvider::new_with_map(
        &action_definition,
        RuntimeResource::Empty,
        get_cached_resource,
    )?);

    // Lookup default skeleton to use since there are no actual resources to probe
    let default_skeleton = default_action_resources
        .resources
        .iter()
        .find_map(|x| match x {
            RuntimeResource::Skeleton(skeleton) => Some(skeleton.clone()),
            _ => None,
        });

    Ok((
        action_definition,
        default_skeleton,
        default_action_resources,
    ))
}

fn create_instance(
    definition: Arc<GraphDefinition>,
    skeleton: Option<Arc<Skeleton>>,
    resources: Arc<dyn GraphResourceProvider>,
) -> Graph {
    if let Some(skeleton) = skeleton {
        definition.build(resources.clone(), skeleton)
    } else {
        definition.build_with_empty_skeleton(resources.clone())
    }
}

static RESOURCES: Mutex<GlobalResources> = Mutex::new(GlobalResources::new());

fn get_cached_resource(_resource_type: &str, value: Value) -> anyhow::Result<RuntimeResource> {
    let res = serde_json::from_value(value)?;
    Ok(RESOURCES.lock().expect("Single threaded").get_cached(res))
}

enum RuntimeResource {
    Empty,
    AnimationClip(AnimationClip),
    BoneGroup(Arc<BoneGroup>),
    Skeleton(Arc<Skeleton>),
}

impl ResourceType for RuntimeResource {
    fn get_resource(&self) -> &dyn std::any::Any {
        match self {
            RuntimeResource::AnimationClip(res) => res,
            RuntimeResource::BoneGroup(res) => res,
            RuntimeResource::Skeleton(res) => res,
            RuntimeResource::Empty => &(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SerializedResource {
    AnimationClip(String),
    BoneGroup(Vec<String>),
    Skeleton(BTreeMap<String, String>),
}

struct Animation {
    pub name: String,
}

struct GlobalResources {
    pub animations: Vec<Animation>,
    pub bone_groups: Vec<Arc<BoneGroup>>,
    pub skeletons: Vec<Arc<Skeleton>>,
}

impl GlobalResources {
    pub const fn new() -> Self {
        Self {
            animations: vec![],
            bone_groups: vec![],
            skeletons: vec![],
        }
    }

    pub fn get_cached(&mut self, serialized: SerializedResource) -> RuntimeResource {
        match serialized {
            SerializedResource::AnimationClip(name) => {
                let looping = name.contains("looping");
                let animation =
                    if let Some(index) = self.animations.iter().position(|x| x.name == name) {
                        AnimationId(index as _)
                    } else {
                        let index = AnimationId(self.animations.len() as _);
                        self.animations.push(Animation { name });
                        index
                    };

                RuntimeResource::AnimationClip(AnimationClip {
                    animation,
                    bone_group: BoneGroupId::All,
                    looping,
                    start: Seconds(0.0),
                    duration: Seconds(1.0),
                })
            }
            SerializedResource::BoneGroup(mut group) => {
                group.sort();
                let mut bones = BoneGroup::new(0, group.as_slice().iter());
                let res = if let Some(res) =
                    self.bone_groups.iter().find(|x| x.weights == bones.weights)
                {
                    res.clone()
                } else {
                    bones.group = BoneGroupId::Reference(self.bone_groups.len() as _);
                    let res = Arc::new(bones);
                    self.bone_groups.push(res.clone());
                    res
                };

                RuntimeResource::BoneGroup(res)
            }
            SerializedResource::Skeleton(map) => {
                let mut skeleton = Skeleton::from_parent_map(&map);
                let res = if let Some(res) = self
                    .skeletons
                    .iter()
                    .find(|x| x.bones == skeleton.bones && x.parents == skeleton.parents)
                {
                    res.clone()
                } else {
                    skeleton.id = SkeletonId(self.skeletons.len() as _);
                    let res = Arc::new(skeleton);
                    self.skeletons.push(res.clone());
                    res
                };
                RuntimeResource::Skeleton(res)
            }
        }
    }
}

fn serialize_upper_body_mask() -> Value {
    serde_json::to_value(SerializedResource::BoneGroup(vec![
        "LeftArm".to_owned(),
        "LeftForeArm".to_owned(),
        "LeftHand".to_owned(),
        "LeftHandIndex1".to_owned(),
        "LeftHandIndex2".to_owned(),
        "LeftHandIndex3".to_owned(),
        "LeftHandMiddle1".to_owned(),
        "LeftHandMiddle2".to_owned(),
        "LeftHandMiddle3".to_owned(),
        "LeftHandPinky1".to_owned(),
        "LeftHandPinky2".to_owned(),
        "LeftHandPinky3".to_owned(),
        "LeftHandRing1".to_owned(),
        "LeftHandRing2".to_owned(),
        "LeftHandRing3".to_owned(),
        "LeftHandThumb1".to_owned(),
        "LeftHandThumb2".to_owned(),
        "LeftHandThumb3".to_owned(),
        "LeftShoulder".to_owned(),
        "Neck".to_owned(),
        "RightArm".to_owned(),
        "RightForeArm".to_owned(),
        "RightHand".to_owned(),
        "RightHandIndex1".to_owned(),
        "RightHandIndex2".to_owned(),
        "RightHandIndex3".to_owned(),
        "RightHandMiddle1".to_owned(),
        "RightHandMiddle2".to_owned(),
        "RightHandMiddle3".to_owned(),
        "RightHandPinky1".to_owned(),
        "RightHandPinky2".to_owned(),
        "RightHandPinky3".to_owned(),
        "RightHandRing1".to_owned(),
        "RightHandRing2".to_owned(),
        "RightHandRing3".to_owned(),
        "RightHandThumb1".to_owned(),
        "RightHandThumb2".to_owned(),
        "RightHandThumb3".to_owned(),
        "RightShoulder".to_owned(),
        "Spine".to_owned(),
        "Spine1".to_owned(),
        "Spine2".to_owned(),
    ]))
    .unwrap()
}

fn serialize_skeleton() -> Value {
    let bones: BTreeMap<_, _> = [
        ("Hips", ""),
        ("Head", "Neck"),
        ("LeftArm", "LeftShoulder"),
        ("LeftFoot", "LeftLeg"),
        ("LeftForeArm", "LeftArm"),
        ("LeftHand", "LeftForeArm"),
        ("LeftHandIndex1", "LeftHand"),
        ("LeftHandIndex2", "LeftHandIndex1"),
        ("LeftHandIndex3", "LeftHandIndex2"),
        ("LeftHandMiddle1", "LeftHand"),
        ("LeftHandMiddle2", "LeftHandMiddle1"),
        ("LeftHandMiddle3", "LeftHandMiddle2"),
        ("LeftHandPinky1", "LeftHand"),
        ("LeftHandPinky2", "LeftHandPinky1"),
        ("LeftHandPinky3", "LeftHandPinky2"),
        ("LeftHandRing1", "LeftHand"),
        ("LeftHandRing2", "LeftHandRing1"),
        ("LeftHandRing3", "LeftHandRing2"),
        ("LeftHandThumb1", "LeftHand"),
        ("LeftHandThumb2", "LeftHandThumb1"),
        ("LeftHandThumb3", "LeftHandThumb2"),
        ("LeftLeg", "LeftUpLeg"),
        ("LeftShoulder", "Spine2"),
        ("LeftToeBase", "LeftFoot"),
        ("LeftUpLeg", "Hips"),
        ("Neck", "Spine2"),
        ("RightArm", "RightShoulder"),
        ("RightFoot", "RightLeg"),
        ("RightForeArm", "RightArm"),
        ("RightHand", "RightForeArm"),
        ("RightHandIndex1", "RightHand"),
        ("RightHandIndex2", "RightHandIndex1"),
        ("RightHandIndex3", "RightHandIndex2"),
        ("RightHandMiddle1", "RightHand"),
        ("RightHandMiddle2", "RightHandMiddle1"),
        ("RightHandMiddle3", "RightHandMiddle2"),
        ("RightHandPinky1", "RightHand"),
        ("RightHandPinky2", "RightHandPinky1"),
        ("RightHandPinky3", "RightHandPinky2"),
        ("RightHandRing1", "RightHand"),
        ("RightHandRing2", "RightHandRing1"),
        ("RightHandRing3", "RightHandRing2"),
        ("RightHandThumb1", "RightHand"),
        ("RightHandThumb2", "RightHandThumb1"),
        ("RightHandThumb3", "RightHandThumb2"),
        ("RightLeg", "RightUpLeg"),
        ("RightShoulder", "Spine2"),
        ("RightToeBase", "RightFoot"),
        ("RightUpLeg", "Hips"),
        ("Spine", "Hips"),
        ("Spine1", "Spine"),
        ("Spine2", "Spine1"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();

    serde_json::to_value(SerializedResource::Skeleton(bones)).unwrap()
}

fn serialize_animation(id: &str) -> Value {
    serde_json::to_value(SerializedResource::AnimationClip(id.to_owned())).unwrap()
}

fn init_res<'a>(list: impl IntoIterator<Item = &'a str>) -> Vec<ResourceContent> {
    list.into_iter()
        .map(|id| {
            if id == SKELETON {
                ResourceContent {
                    name: id.to_owned(),
                    resource_type: Skeleton::resource_type().to_owned(),
                    content: serialize_skeleton(),
                    ..Default::default()
                }
            } else if id == UPPER_BODY_MASK {
                ResourceContent {
                    name: id.to_owned(),
                    resource_type: BoneGroup::resource_type().to_owned(),
                    content: serialize_upper_body_mask(),
                    ..Default::default()
                }
            } else {
                ResourceContent {
                    name: id.to_owned(),
                    resource_type: AnimationClip::resource_type().to_owned(),
                    content: serialize_animation(id),
                    ..Default::default()
                }
            }
        })
        .collect()
}

fn init(name: &str, value: impl Into<InitialParameterValue>) -> Parameter {
    Parameter {
        name: name.to_owned(),
        initial: value.into(),
        ..Default::default()
    }
}

const IDLE_CLIP: &str = "idle_looping";
const WALKING_CLIP: &str = "walking_looping";
const RUNNING_CLIP: &str = "running_looping";
const JUMPING_CLIP: &str = "jumping_looping";
const FALLING_CLIP: &str = "falling_looping";
const DANCING_CLIP: &str = "dancing_looping";
const DANCING_UPPER_CLIP: &str = "dancing_upper_looping";
const SITTING_CLIP: &str = "sitting_looping";
const POINTING_FORWARD_CLIP: &str = "pointing_forward";
const TURN_AND_SIT_CLIP: &str = "turn_and_sit";
const UPPER_BODY_MASK: &str = "upper_body_mask";
const SKELETON: &str = "skeleton";
const TRANSITION_DURATION: Seconds = Seconds(0.2);
const FADE_OUT_DURATION: Seconds = Seconds(0.4);
const ACTION_EVENT: &str = "action";
const UPPER_BODY_ACTION_EVENT: &str = "upper_body_action";

fn create_locomotion_graph() -> AnimGraph {
    let resources = init_res([
        SKELETON,
        IDLE_CLIP,
        WALKING_CLIP,
        RUNNING_CLIP,
        JUMPING_CLIP,
        FALLING_CLIP,
        DANCING_CLIP,
        DANCING_UPPER_CLIP,
        UPPER_BODY_MASK,
    ]);

    let parameters = vec![
        init("grounded", true),
        init("falling", true),
        init("locomotion_speed", 0.0f32),
        init("overdrive", 1.0f32),
        init("moving", false),
        init("dance", false),
    ];

    let locomotion_layer = {
        let idle_node = alias("Idle", animation_pose(IDLE_CLIP));
        let walking_node = alias("Walking", animation_pose(WALKING_CLIP));
        let running_node = alias("Running", animation_pose(RUNNING_CLIP));

        let locomotion_blend = endpoint(alias(
            "locomotion",
            blend_ranges(
                bind_parameter("locomotion_speed"),
                [(0.0, idle_node), (2.0, walking_node), (6.0, running_node)],
            ),
        ));

        let speed_scaled_locomotion = preprocess(
            speed_scale(bind_parameter("overdrive"), ALPHA_ONE),
            locomotion_blend,
        );

        state_machine(
            "Locomotion",
            [state("Blend").with(speed_scaled_locomotion, [])],
        )
    };

    let jump_layer = {
        const STATE_FALLING: &str = "Falling";
        const STATE_JUMPING: &str = "Jumping";
        const STATE_OFF: &str = "Off";

        let not_grounded = bind_parameter::<bool>("grounded").not();
        let has_fallen = bind_parameter::<bool>("falling").and(not_grounded.clone());

        let state_off = state(STATE_OFF)
            .with_branch(endpoint(inactive_layer()))
            .with_transitions([
                has_fallen.transition(STATE_FALLING, TRANSITION_DURATION),
                not_grounded.transition(STATE_JUMPING, TRANSITION_DURATION),
            ]);

        let is_grounded = bind_parameter::<bool>("grounded").into_expr();
        let is_falling = bind_parameter::<bool>("falling").into_expr();
        let state_jumping = state(STATE_JUMPING)
            .with_branch(endpoint(tree([
                animation_pose(JUMPING_CLIP),
                state_event("jumped", true, EventEmit::Entry),
            ])))
            .with_transitions([
                is_grounded
                    .clone()
                    .transition(STATE_OFF, TRANSITION_DURATION),
                is_falling.transition(STATE_FALLING, TRANSITION_DURATION),
            ]);

        let state_falling = state(STATE_FALLING)
            .with_branch(endpoint(animation_pose(FALLING_CLIP)))
            .with_transitions([is_grounded.transition(STATE_OFF, TRANSITION_DURATION)]);

        state_machine("Jump", [state_off, state_jumping, state_falling])
    };

    let dance_layer = {
        const STATE_OFF: &str = "Off";
        const STATE_FULL: &str = "Full Body";
        const STATE_UPPER: &str = "Upper Body";

        let is_dancing =
            bind_parameter::<bool>("dance").and(state_is("::Dance::Off", QueryType::Entered));
        let is_moving_and_dancing = bind_parameter::<bool>("moving")
            .or(bind_route::<bool>("action_active"))
            .and(is_dancing.clone());
        let state_off = state(STATE_OFF)
            .with_branch(endpoint(inactive_layer()))
            .with_transitions([
                is_moving_and_dancing.transition(STATE_UPPER, TRANSITION_DURATION),
                is_dancing.transition(STATE_FULL, TRANSITION_DURATION),
            ]);

        let state_full = {
            let dancing_pose = endpoint(animation_pose(DANCING_CLIP));
            let not_dancing = bind_parameter::<bool>("dance").not();
            let is_moving = bind_parameter::<bool>("moving");
            state(STATE_FULL)
                .with_branch(dancing_pose)
                .with_transitions([
                    not_dancing.transition(STATE_OFF, FADE_OUT_DURATION),
                    is_moving.transition(STATE_UPPER, TRANSITION_DURATION),
                ])
        };

        let state_upper = {
            let dancing_upper = endpoint(tree([
                animation_pose(DANCING_UPPER_CLIP),
                pose_mask(UPPER_BODY_MASK),
            ]));
            let is_idle_or_not_dancing =
                bind_parameter::<bool>("dance")
                    .not()
                    .or(bind_parameter::<bool>("moving")
                        .not()
                        .and(bind_route::<bool>("action_active").not()));

            state(STATE_UPPER)
                .with_branch(dancing_upper)
                .with_transitions([is_idle_or_not_dancing.transition(STATE_OFF, FADE_OUT_DURATION)])
        };

        state_machine("Dance", [state_off, state_upper, state_full])
    };

    let action_layer = {
        const STATE_OFF: &str = "Off";
        const STATE_FULL: &str = "Full Body";
        const STATE_UPPER: &str = "Upper Body";

        state_machine(
            "Action",
            [
                state(STATE_OFF)
                    .with_branch(endpoint(inactive_layer()))
                    .with_transitions([
                        bind_route::<bool>("action_active")
                            .transition(STATE_FULL, TRANSITION_DURATION),
                        bind_route::<bool>("upper_body_action_active")
                            .transition(STATE_UPPER, TRANSITION_DURATION),
                    ]),
                state(STATE_FULL)
                    .with_branch(endpoint(reference_pose()))
                    .with_transitions([bind_route::<bool>("action_active")
                        .not()
                        .or(bind_parameter::<bool>("moving"))
                        .transition(STATE_OFF, TRANSITION_DURATION)]),
                state(STATE_UPPER)
                    .with_branch(endpoint(tree([
                        reference_pose(),
                        pose_mask(UPPER_BODY_MASK),
                    ])))
                    .with_transitions([bind_route::<bool>("upper_body_action_active")
                        .not()
                        .or(bind_route::<bool>("action_active"))
                        .or(bind_parameter::<bool>("moving"))
                        .transition(STATE_OFF, FADE_OUT_DURATION * 2.0)]),
            ],
        )
    };

    let root = state_machine(
        "Root",
        [state("Layers").with_layers([
            submachine("Locomotion"),
            submachine("Jump"),
            preprocess(
                tree([
                    alias("action_active", event_emitted(ACTION_EVENT)),
                    alias(
                        "upper_body_action_active",
                        event_emitted(UPPER_BODY_ACTION_EVENT),
                    ),
                ]),
                submachine("Action"),
            ),
            submachine("Dance"),
        ])],
    );

    AnimGraph {
        resources,
        parameters,
        state_machines: vec![
            root,
            locomotion_layer,
            jump_layer,
            action_layer,
            dance_layer,
        ],
        ..Default::default()
    }
}

fn create_action_graph() -> AnimGraph {
    let resources = init_res([
        SKELETON,
        SITTING_CLIP,
        TURN_AND_SIT_CLIP,
        POINTING_FORWARD_CLIP,
        UPPER_BODY_MASK,
    ]);

    let parameters = vec![
        init(
            "fade_out",
            InitialParameterValue::Event(FADE_OUT_EVENT.into()),
        ),
        init("turn_and_sit", false),
        init("sit", InitialParameterValue::Event(SIT_EVENT.into())),
        init("point_of_interest", InitialParameterValue::Vector([0.0; 3])),
        init("enable_point_of_interest", false),
        init(
            "poi_cooldown",
            InitialParameterValue::Event(COOLDOWN_EVENT.into()),
        ),
    ];

    const COOLDOWN_EVENT: &str = "cooldown";
    const FADE_OUT_EVENT: &str = "fade_out";
    const SIT_EVENT: &str = "sit";

    const POINTED_EVENT: &str = "pointed";

    let pointing_cooldown = endpoint(tree([
        reference_pose(),
        state_event(COOLDOWN_EVENT, true, EventEmit::Entry),
        alias("bounce_back", blend_in(ALPHA_ZERO, Seconds(5.0))),
    ]));

    let off_pose = endpoint(tree([
        reference_pose(),
        alias(
            "poi_offset",
            transform_offset("Hips", bind_parameter("point_of_interest")),
        ),
    ]));

    const EMIT_ON_ENTER: EventEmit = EventEmit::Or(FlowState::Entering, FlowState::Entered);

    let sitting_pose = endpoint(tree([
        animation_pose(SITTING_CLIP),
        state_event(ACTION_EVENT, true, EMIT_ON_ENTER),
    ]));
    let turn_and_sit_pose = endpoint(tree([
        animation_pose(TURN_AND_SIT_CLIP),
        state_event(ACTION_EVENT, true, EMIT_ON_ENTER),
    ]));
    let pointing_forward = endpoint(tree([
        animation_pose(POINTING_FORWARD_CLIP),
        remaining_event(
            bind_route("animation_pose"),
            POINTED_EVENT,
            true,
            TRANSITION_DURATION,
            EventEmit::Never,
        ),
        state_event(UPPER_BODY_ACTION_EVENT, true, EMIT_ON_ENTER),
    ]));
    use QueryType::*;

    const POINTING_STATE: &str = "Pointing";
    const COOLDOWN_STATE: &str = "Cooldown";
    let idling = state_machine(
        "Idle",
        [
            state(OFF_STATE).with_branch(off_pose).with_transitions([
                bind_route::<f32>("bounce_back")
                    .not_equal(1.0)
                    .immediate_transition(COOLDOWN_STATE),
                bind_parameter::<bool>("enable_point_of_interest")
                    .and(contains_inclusive(
                        (0.4, 1.5),
                        bind_route::<[f32; 3]>("poi_offset").projection(Projection::Length),
                    ))
                    .and(
                        bind_route::<[f32; 3]>("poi_offset")
                            .projection(Projection::Back)
                            .gt(0.1),
                    )
                    .transition(POINTING_STATE, FADE_OUT_DURATION),
            ]),
            state(POINTING_STATE)
                .with_branch(pointing_forward)
                .with_transitions([bind_parameter::<bool>("enable_point_of_interest")
                    .not()
                    .or(event_is(POINTED_EVENT, QueryType::Active))
                    .transition(COOLDOWN_STATE, FADE_OUT_DURATION * 2.0)]),
            state(COOLDOWN_STATE)
                .with_branch(pointing_cooldown)
                .with_transitions([bind_route::<f32>("bounce_back")
                    .equals(1.0)
                    .immediate_transition(OFF_STATE)]),
        ],
    );

    const OFF_STATE: &str = "Off";
    const TURN_AND_SIT_STATE: &str = "Turn and sit";
    const SITTING_STATE: &str = "Sitting";

    let root = state_machine(
        "Root",
        [
            state(OFF_STATE)
                .with_branch(submachine("Idle"))
                .with_transitions([
                    event_is(FADE_OUT_EVENT, Exited)
                        .and(event_is(SIT_EVENT, Active))
                        .transition(SITTING_STATE, TRANSITION_DURATION),
                    event_is(FADE_OUT_EVENT, Exited)
                        .and(bind_parameter::<bool>("turn_and_sit"))
                        .transition(TURN_AND_SIT_STATE, TRANSITION_DURATION),
                ]),
            state(SITTING_STATE)
                .with_branch(sitting_pose)
                .with_global_condition(
                    event_is(FADE_OUT_EVENT, Exited).and(event_is(SIT_EVENT, Active)),
                )
                .with_transitions([event_is(SIT_EVENT, Exited)
                    .into_expr()
                    .transition(OFF_STATE, FADE_OUT_DURATION)]),
            state(TURN_AND_SIT_STATE)
                .with_branch(turn_and_sit_pose)
                .with_global_condition(
                    event_is(FADE_OUT_EVENT, Exited).and(bind_parameter::<bool>("turn_and_sit")),
                )
                .with_transitions([
                    event_is(FADE_OUT_EVENT, Active)
                        .into_expr()
                        .transition(OFF_STATE, FADE_OUT_DURATION),
                    event_is(SIT_EVENT, Active)
                        .into_expr()
                        .transition(SITTING_STATE, TRANSITION_DURATION),
                ]),
        ],
    );

    AnimGraph {
        resources,
        parameters,
        state_machines: vec![root, idling],
        ..Default::default()
    }
}
