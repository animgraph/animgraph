use std::{
    fmt::Debug,
    num::NonZeroU16,
    sync::{Arc, Mutex},
};

use glam::Vec3;
use serde_derive::{Deserialize, Serialize};

use crate::{
    io::Event,
    state_machine::{
        ConstantIndex, HierarchicalTransition, MachineIndex, NodeIndex, StateIndex, VariableIndex,
    },
    unorm_clamped, Alpha, BooleanVec, FlowState, GraphCondition, GraphDefinition, GraphExpression,
    GraphNodeEntry, GraphNumberExpression, GraphResourceProvider, GraphResourceRef, Id, IndexType,
    InterpreterContext, NumberOperation, NumberRange, OptionalIndexType, Projection, SampleTimer,
    Seconds, Skeleton, Transform, ALPHA_ONE, ALPHA_ZERO,
};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphBoolean {
    #[default]
    Never,
    Always,
    Variable(VariableIndex),
    Condition(IndexType),
    QueryMachineActive(MachineIndex),
    QueryMachineState(FlowState, MachineIndex),
    QueryStateActive(StateIndex),
    QueryState(FlowState, StateIndex),
    QueryEventActive(Event),
    QueryEvent(FlowState, Event),
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNumber {
    #[default]
    Zero,
    One,
    Iteration,
    Projection(Projection, VariableIndex),
    Constant(ConstantIndex),
    Variable(VariableIndex),
    Expression(IndexType),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GraphDebugBreak {
    Condition { condition_index: IndexType },
}

pub trait FlowEventsHook: Send {
    fn debug_trigger(
        &mut self,
        graph: &Graph,
        context: &InterpreterContext,
        debug_break: GraphDebugBreak,
        result: bool,
    );
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTransitionState {
    // Transition Node Tree
    pub node: Option<NodeIndex>,
    // This state is currently transitioning to
    pub next: Option<StateIndex>,
    // This state is currently being transitioned into by
    pub transition_index: Option<IndexType>,
    pub blend: Alpha,
    pub completion: Alpha,
    pub duration: Seconds,
}

impl Default for GraphTransitionState {
    fn default() -> Self {
        Self {
            node: None,
            next: None,
            transition_index: None,
            blend: ALPHA_ZERO,
            completion: ALPHA_ONE,
            duration: Seconds(0.0),
        }
    }
}

impl GraphTransitionState {
    pub fn get_state(&self) -> FlowState {
        if self.next.is_some() {
            return FlowState::Exiting;
        }

        if self.transition_index.is_some() {
            return FlowState::Entering;
        }

        if self.blend == ALPHA_ZERO {
            FlowState::Exited
        } else {
            FlowState::Entered
        }
    }

    pub fn init(
        &mut self,
        blend: Alpha,
        completion: Alpha,
        duration: Seconds,
        transition_index: IndexType,
    ) {
        assert!(self.next.is_none());
        assert!(self.transition_index.is_none());

        self.transition_index = Some(transition_index);
        self.blend = blend;
        self.completion = completion;
        self.duration = duration;
    }

    pub fn complete(&mut self) {
        self.transition_index = None;
        self.blend = ALPHA_ONE;
        self.completion = ALPHA_ONE;
    }

    pub fn update(&mut self, delta_time: Seconds) {
        if self.duration.0 > 0.0 {
            let dt = delta_time.0 / self.duration.0;
            self.completion = unorm_clamped(self.completion.0 + dt);
        }
    }

    pub fn is_complete(&self) -> bool {
        self.completion.is_nearly_one()
    }

    pub fn reset(&mut self) {
        *self = Default::default();
    }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct ActiveStates {
    first: OptionalIndexType,
    last: OptionalIndexType,
}

impl ActiveStates {
    pub fn get_active(&self) -> Option<StateIndex> {
        self.last.map(|x| StateIndex(x.get() - 1))
    }

    pub fn set_active(&mut self, index: StateIndex) {
        self.last = NonZeroU16::new(index.0 + 1);
    }

    pub fn reset(&mut self) {
        self.first = None;
        self.last = None;
    }

    pub fn get_start(&self) -> Option<StateIndex> {
        self.first.map(|x| StateIndex(x.get() - 1))
    }

    pub fn set_start(&mut self, index: StateIndex) {
        self.first = NonZeroU16::new(index.0 + 1);
    }

    pub fn start_transition(&mut self, index: StateIndex) {
        if self.first.is_none() {
            self.set_start(index);
        }
    }

    pub fn reset_start(&mut self) {
        self.first = None;
    }
}

/// The persistent data of a `HierarchalStateMachine` evaluated by `FlowGraphRunner`
///
/// WIP Notes / Things likely to change if time permits:
/// * The ideal lifetime model would be probably be arena allocated based.
/// * Dyn traits might go away in favour of enum dispatch.
pub struct Graph {
    active_machine_states: Vec<ActiveStates>,
    state_transition: Vec<GraphTransitionState>,
    iteration: f64,
    timers: Vec<SampleTimer>,
    booleans: BooleanVec,
    numbers: Vec<f32>,
    states: Vec<FlowState>,
    events: Vec<FlowState>,
    resources: Vec<GraphResourceRef>,
    pose: Vec<Transform>,
    root_to_world: Transform,

    // Unserializable
    definition: Arc<GraphDefinition>,
    skeleton: Arc<Skeleton>,

    resource_provider: Arc<dyn GraphResourceProvider>,
    events_hook: Mutex<Option<Box<dyn FlowEventsHook>>>,
}

impl Graph {
    pub(crate) fn new_internal(
        definition: Arc<GraphDefinition>,
        resources: Vec<GraphResourceRef>,
        provider: Arc<dyn GraphResourceProvider>,
        skeleton: Arc<Skeleton>,
    ) -> Graph {
        Self {
            active_machine_states: vec![Default::default(); definition.desc().total_machines()],
            state_transition: vec![
                GraphTransitionState::default();
                definition.desc().total_states()
            ],
            resources,
            resource_provider: provider,
            states: vec![FlowState::Exited; definition.desc().total_states()],
            events: vec![FlowState::Exited; definition.total_events()],
            booleans: vec![false; definition.total_boolean_variables()],
            numbers: vec![0.0; definition.total_number_variables()],
            timers: vec![SampleTimer::FIXED; definition.total_timers()],
            definition,
            skeleton,
            pose: vec![],
            root_to_world: Transform::default(),
            events_hook: Default::default(),
            iteration: 0.0,
        }
    }

    pub fn reset_graph(&mut self) {
        let Self {
            active_machine_states,
            state_transition,
            iteration,
            timers,
            booleans,
            numbers,
            states,
            events,
            resources,
            definition,
            resource_provider,
            events_hook,
            skeleton,
            pose,
            root_to_world,
        } = self;

        active_machine_states.fill(Default::default());
        state_transition.fill(Default::default());
        resource_provider.initialize(resources, definition);
        states.fill(FlowState::Exited);
        events.fill(FlowState::Exited);
        booleans.fill(false);
        numbers.fill(0.0);
        timers.fill(SampleTimer::FIXED);
        pose.clear();
        *root_to_world = Default::default();
        *iteration = 0.0;
        let _ = events_hook;
        let _ = skeleton;

        Arc::clone(definition).reset_parameters(self);
    }

    pub fn resources(&self) -> &Arc<dyn GraphResourceProvider> {
        &self.resource_provider
    }

    pub fn skeleton(&self) -> &Arc<Skeleton> {
        &self.skeleton
    }

    pub fn transform_by_id(&self, bone: Id) -> Option<&Transform> {
        self.skeleton.transform_by_id(&self.pose, bone)
    }

    pub fn set_transforms(
        &mut self,
        bone_to_root: impl IntoIterator<Item = Transform>,
        root_to_world: &Transform,
    ) {
        self.root_to_world = root_to_world.clone();
        self.pose.clear();
        self.pose.extend(bone_to_root);
    }

    pub fn get_root_to_world(&self) -> &Transform {
        &self.root_to_world
    }

    pub fn pose(&self) -> &[Transform] {
        &self.pose
    }

    pub fn get_state(&self, index: StateIndex) -> FlowState {
        *self
            .states
            .get(index.0 as usize)
            .unwrap_or(&FlowState::default())
    }

    pub fn get_event(&self, index: Event) -> FlowState {
        *self
            .events
            .get(index.0 as usize)
            .unwrap_or(&FlowState::default())
    }

    pub fn set_event(&mut self, index: Event, state: FlowState) -> FlowState {
        if let Some(event) = self.events.get_mut(index.0 as usize) {
            let result = *event;
            *event = state;
            result
        } else {
            FlowState::default()
        }
    }

    pub fn is_state_active(&self, index: StateIndex) -> bool {
        self.states.get(index.0 as usize).map(|x| x.is_active()) == Some(true)
    }

    pub fn with_timer_mut<T>(
        &mut self,
        index: IndexType,
        f: impl FnOnce(&mut SampleTimer) -> T,
    ) -> Option<T> {
        self.timers.get_mut(index as usize).map(f)
    }

    pub fn with_timer<T>(&self, index: IndexType, f: impl FnOnce(&SampleTimer) -> T) -> Option<T> {
        self.timers.get(index as usize).map(f)
    }

    pub fn set_events_hook(&mut self, hook: Box<dyn FlowEventsHook>) {
        self.events_hook = Mutex::new(Some(hook));
    }

    pub fn try_get_node(&self, index: NodeIndex) -> Option<&GraphNodeEntry> {
        self.definition.get_node(index)
    }

    pub fn get_bool(&self, value: GraphBoolean) -> bool {
        match value {
            GraphBoolean::Never => false,
            GraphBoolean::Always => true,
            GraphBoolean::Condition(index) => {
                static NO_CONTEXT: InterpreterContext = InterpreterContext::new_inactive();
                self.eval_condition_index(&NO_CONTEXT, index)
            }
            GraphBoolean::Variable(index) => self.booleans.get(index.0 as usize) == Some(&true),
            GraphBoolean::QueryMachineState(state, machine) => {
                self.get_machine_state(machine)
                    .map(|index| self.get_state(index))
                    == Some(state)
            }
            GraphBoolean::QueryMachineActive(index) => self.get_machine_state(index).is_some(),

            GraphBoolean::QueryState(state, index) => self.get_state(index) == state,
            GraphBoolean::QueryStateActive(index) => self.get_state(index).is_active(),

            GraphBoolean::QueryEvent(state, index) => self.get_event(index) == state,
            GraphBoolean::QueryEventActive(index) => self.get_event(index).is_active(),
        }
    }

    pub fn get_variable_boolean(&self, index: VariableIndex) -> bool {
        *self.booleans.get(index.0 as usize).unwrap_or(&false)
    }

    pub fn set_variable_boolean(&mut self, index: VariableIndex, input: bool) {
        *self
            .booleans
            .get_mut(index.0 as usize)
            .unwrap_or(&mut false) = input;
    }

    fn eval_number(&self, value: GraphNumber, max_index: usize, list: &[GraphExpression]) -> f64 {
        match value {
            GraphNumber::Zero => 0.0,
            GraphNumber::One => 1.0,
            GraphNumber::Iteration => self.iteration,
            GraphNumber::Projection(projection, index) => {
                projection.character_projected(self.get_vec3(index)) as f64
            }
            GraphNumber::Constant(index) => self.definition.get_constant_number(index),
            GraphNumber::Variable(index) => self.get_variable_number(index) as f64,
            GraphNumber::Expression(index) => {
                assert!(index as usize > max_index, "Recursive number expression.");
                self.eval_number_expression(index as usize, list)
            }
        }
    }

    fn eval_number_expression(&self, index: usize, list: &[GraphExpression]) -> f64 {
        match list[index].expression().clone() {
            GraphNumberExpression::Binary(op, lhs, rhs) => {
                let a = self.eval_number(lhs, index, list);
                let b = self.eval_number(rhs, index, list);

                match op {
                    NumberOperation::Add => a + b,
                    NumberOperation::Subtract => a - b,
                    NumberOperation::Divide => a / b,
                    NumberOperation::Multiply => a * b,
                    NumberOperation::Modulus => a % b,
                }
            }
        }
    }

    pub fn get_number(&self, value: GraphNumber) -> f64 {
        match value {
            GraphNumber::Zero => 0.0,
            GraphNumber::One => 1.0,
            GraphNumber::Iteration => self.iteration,
            GraphNumber::Projection(projection, index) => {
                projection.character_projected(self.get_vec3(index)) as f64
            }
            GraphNumber::Constant(index) => self.definition.get_constant_number(index),
            GraphNumber::Variable(index) => self.get_variable_number(index) as f64,
            GraphNumber::Expression(index) => {
                let expressions = &self.definition().desc().expressions;
                self.eval_number_expression(index as usize, expressions)
            }
        }
    }

    pub fn get_variable_number(&self, index: VariableIndex) -> f32 {
        *self.numbers.get(index.0 as usize).unwrap_or(&0.0)
    }

    pub fn set_variable_number(&mut self, index: VariableIndex, input: f32) {
        *self.numbers.get_mut(index.0 as usize).unwrap_or(&mut 0.0) = input;
    }

    pub fn set_variable_number_array<const N: usize>(
        &mut self,
        index: VariableIndex,
        values: [f32; N],
    ) {
        self.numbers
            .get_mut(index.0 as usize..index.0 as usize + N)
            .unwrap_or(&mut [0.0; N])
            .copy_from_slice(&values);
    }

    pub fn get_variable_number_array<const N: usize>(&self, index: VariableIndex) -> [f32; N] {
        let mut result: [f32; N] = [0.0; N];
        result.copy_from_slice(
            self.numbers
                .get(index.0 as usize..index.0 as usize + N)
                .unwrap_or(&[0.0; N]),
        );
        result
    }

    pub fn get_vec3(&self, index: VariableIndex) -> Vec3 {
        self.get_variable_number_array(index).into()
    }

    pub fn definition(&self) -> &Arc<GraphDefinition> {
        &self.definition
    }

    pub fn iteration(&self) -> u64 {
        self.iteration as u64
    }

    pub fn get_first_transitioning_state(&self, machine: MachineIndex) -> Option<StateIndex> {
        self.active_machine_states
            .get(machine.0 as usize)
            .and_then(|x| x.get_start())
    }

    pub fn get_next_transitioning_state(&self, state: StateIndex) -> Option<StateIndex> {
        self.state_transition
            .get(state.0 as usize)
            .and_then(|x| x.next)
    }

    pub fn get_machine_state(&self, machine: MachineIndex) -> Option<StateIndex> {
        self.active_machine_states
            .get(machine.0 as usize)
            .and_then(|x| x.get_active())
    }

    pub fn get_resource<T: 'static>(&self, index: IndexType) -> Option<&T> {
        self.resources.get(index as usize).and_then(|&resource| {
            let any: &dyn std::any::Any = self.resource_provider.get(resource);
            any.downcast_ref::<T>()
        })
    }

    pub fn get_state_transition(&self, state: StateIndex) -> Option<&GraphTransitionState> {
        self.state_transition.get(state.0 as usize)
    }

    pub fn get_state_transition_mut(
        &mut self,
        state: StateIndex,
    ) -> Option<&mut GraphTransitionState> {
        self.state_transition.get_mut(state.0 as usize)
    }
}

impl Graph {
    pub(crate) fn tick(&mut self) {
        self.iteration += 1.;
    }

    pub(crate) fn reset_machine_state(&mut self, machine: MachineIndex) {
        let chain_range = self
            .definition
            .get_machine_states(machine)
            .expect("Valid machine");
        let chain_range = chain_range.start as usize..chain_range.end as usize;

        let transition_states = self
            .state_transition
            .get_mut(chain_range)
            .expect("Valid stack");

        for state in transition_states {
            state.reset();
        }

        if let Some(active_state) = self.active_machine_states.get_mut(machine.0 as usize) {
            active_state.reset();
        };
    }

    pub(crate) fn set_machine_state(
        &mut self,
        machine: MachineIndex,
        target: StateIndex,
        entered_state: FlowState,
    ) {
        self.active_machine_states
            .get_mut(machine.0 as usize)
            .expect("Valid machine")
            .set_active(target);

        self.set_state(target, entered_state);

        if entered_state == FlowState::Entered {
            let transition = &mut self.state_transition[target.0 as usize];
            assert!(transition.next.is_none());
            assert!(transition.transition_index.is_none());
            transition.complete();
        }
    }

    fn debug_trigger(
        &self,
        context: &InterpreterContext,
        debug_break: GraphDebugBreak,
        result: bool,
    ) {
        let mut lock = self.events_hook.lock().expect("Valid lock");
        if let Some(hook) = lock.as_mut() {
            hook.debug_trigger(self, context, debug_break, result);
        }
    }

    fn eval_condition_index(&self, context: &InterpreterContext, index: IndexType) -> bool {
        let Some(condition) = self.definition.get_subcondition(index) else {
            return false;
        };

        let result = self.eval_condition_interal(context, &condition, index as usize + 1);

        if condition.has_debug_break() {
            self.debug_trigger(
                context,
                GraphDebugBreak::Condition {
                    condition_index: index,
                },
                result,
            );
        }
        result
    }

    fn eval_condition_interal(
        &self,
        context: &InterpreterContext,
        condition: &GraphCondition,
        sub_condition_max: usize,
    ) -> bool {
        use crate::ConditionExpression::*;

        match *condition.expression() {
            Never => false,
            Always => true,
            UnaryTrue(value) => self.get_bool(value),
            UnaryFalse(value) => !self.get_bool(value),

            Equal(a, b) => self.get_bool(a) == self.get_bool(b),
            NotEqual(a, b) => self.get_bool(a) != self.get_bool(b),

            Like(a, b) => (self.get_number(a) - self.get_number(b)).abs() < 1e-6,
            NotLike(a, b) => (self.get_number(a) - self.get_number(b)).abs() >= 1e-6,

            Contains(NumberRange::Exclusive(a, b), v) => {
                let x = self.get_number(v);
                let range = self.get_number(a)..self.get_number(b);
                range.contains(&x)
            }
            Contains(NumberRange::Inclusive(a, b), v) => {
                let x = self.get_number(v);
                let range = self.get_number(a)..=self.get_number(b);
                range.contains(&x)
            }
            NotContains(NumberRange::Exclusive(a, b), v) => {
                let x = self.get_number(v);
                let range = self.get_number(a)..self.get_number(b);
                !range.contains(&x)
            }
            NotContains(NumberRange::Inclusive(a, b), v) => {
                let x = self.get_number(v);
                let range = self.get_number(a)..=self.get_number(b);
                !range.contains(&x)
            }
            Ordering(order, a, b) => match self.get_number(a).partial_cmp(&self.get_number(b)) {
                Some(ordering) => order == ordering,
                None => false,
            },
            NotOrdering(order, a, b) => match self.get_number(a).partial_cmp(&self.get_number(b)) {
                Some(ordering) => order != ordering,
                None => false,
            },
            AllOf(a, b, c) => {
                assert!(sub_condition_max <= a as _, "recursive condition");
                c == 'condition: {
                    for index in a..b {
                        if !self.eval_condition_index(context, index) {
                            break 'condition false;
                        }
                    }
                    true
                }
            }
            NoneOf(a, b, c) => {
                assert!(sub_condition_max <= a as _, "recursive condition");
                c == 'condition: {
                    for index in a..b {
                        if self.eval_condition_index(context, index) {
                            break 'condition false;
                        }
                    }
                    true
                }
            }
            AnyTrue(a, b, c) => {
                assert!(sub_condition_max <= a as _, "recursive condition");
                c == 'condition: {
                    for index in a..b {
                        if self.eval_condition_index(context, index) {
                            break 'condition true;
                        }
                    }
                    false
                }
            }
            AnyFalse(a, b, c) => {
                assert!(sub_condition_max <= a as _, "recursive condition");
                c == 'condition: {
                    for index in a..b {
                        if !self.eval_condition_index(context, index) {
                            break 'condition true;
                        }
                    }
                    false
                }
            }
            ExclusiveOr(a, b, c) => {
                assert!(sub_condition_max <= a as _, "recursive condition");
                let mut result = false;
                c == 'condition: {
                    for index in a..b {
                        let expr = self.eval_condition_index(context, index);
                        if expr && result {
                            break 'condition false;
                        }
                        result |= expr;
                    }
                    result
                }
            }
            ExclusiveNot(a, b, c) => {
                assert!(sub_condition_max <= a as _, "recursive condition");
                let mut result = false;
                c == 'condition: {
                    for index in a..b {
                        let expr = !self.eval_condition_index(context, index);
                        if expr && result {
                            break 'condition false;
                        }
                        result |= expr;
                    }
                    result
                }
            }
        }
    }

    pub(crate) fn eval_condition(
        &self,
        context: &InterpreterContext,
        condition: &GraphCondition,
    ) -> bool {
        self.eval_condition_interal(context, condition, 0)
    }

    pub(crate) fn eval_initial_state(&mut self, machine: MachineIndex) -> StateIndex {
        let states = self
            .definition
            .get_machine_states(machine)
            .expect("Machine has states");
        assert!(!states.is_empty(), "Machine has states");

        let mut initial_state = states.start.into();
        let mut ctx = InterpreterContext {
            machine,
            ..Default::default()
        };
        for state in states {
            let state = state.into();
            if let Some(condition) = self.definition.get_state_global_condition(state) {
                ctx.state = state;

                let result = self.eval_condition(&ctx, &condition);
                if result {
                    initial_state = state;
                    break;
                }
            }
        }

        initial_state
    }

    pub(crate) fn init_transition(
        &mut self,
        context: &InterpreterContext,
        transition: &HierarchicalTransition,
        transition_index: IndexType,
    ) -> Alpha {
        let progress = if let Some(progress) = transition.progress {
            progress.get(self)
        } else {
            ALPHA_ZERO
        };

        let blend = if let Some(blend) = transition.blend {
            blend.get(self)
        } else {
            progress
        };

        let duration = if let Some(duration) = transition.duration {
            duration.get(self)
        } else {
            Seconds(0.0)
        };

        let origin = self
            .state_transition
            .get_mut(transition.origin.0 as usize)
            .expect("Valid state index");

        assert!(origin.next.is_none());
        assert!(origin.node.is_none());
        origin.next = Some(transition.target);
        origin.node = transition.node;

        let target = self
            .state_transition
            .get_mut(transition.target.0 as usize)
            .expect("Valid state index");

        target.init(blend, progress, duration, transition_index);

        let active = self
            .active_machine_states
            .get_mut(context.machine.0 as usize)
            .expect("Valid machine");

        active.start_transition(transition.origin);

        target.blend
    }

    pub(crate) fn shutdown_transition(&mut self, context: &InterpreterContext) -> StateIndex {
        let state = self
            .state_transition
            .get_mut(context.state.0 as usize)
            .expect("Valid state index");

        let result = state.next.expect("State in transition");
        assert!(
            state.transition_index.is_none(),
            "State is being transitioned into"
        );
        state.reset();

        let target = self
            .state_transition
            .get_mut(result.0 as usize)
            .expect("Valid state index");

        target.complete();

        let active = self
            .active_machine_states
            .get_mut(context.machine.0 as usize)
            .expect("Valid machine");
        if target.next.is_some() {
            active.set_start(result);
        } else {
            active.reset_start();
        }

        result
    }

    pub(crate) fn get_completed_transition(
        &mut self,
        context: &InterpreterContext,
    ) -> Option<StateIndex> {
        let Some(transitioning_state) = self.get_first_transitioning_state(context.machine) else {
            return None;
        };

        let first = &self.state_transition[transitioning_state.0 as usize];
        assert!(first.transition_index.is_none());
        assert_eq!(first.blend, ALPHA_ONE);

        let mut next = first.next;

        let mut completed = None;

        while let Some(state) = next {
            let transition_state = &mut self.state_transition[state.0 as usize];

            if transition_state.is_complete() {
                completed = Some(state);
            }

            next = transition_state.next;
        }

        completed
    }

    pub(crate) fn set_state(&mut self, state_index: StateIndex, state: FlowState) {
        *self
            .states
            .get_mut(state_index.0 as usize)
            .expect("Valid state") = state;
    }
}
