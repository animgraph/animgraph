use serde_derive::{Deserialize, Serialize};
use std::str::FromStr;

use crate::core::{Seconds, Alpha, ALPHA_ONE, ALPHA_ZERO};
use crate::layer_builder::{LayerBuilder, LayerType, LayerWeight};
use crate::processors::{FlowEvents, GraphVisitor};
use crate::state_machine::{
    HierarchicalBranch, HierarchicalBranchTarget, MachineIndex, NodeIndex, StateIndex,
};
use crate::IndexType;
use crate::{Graph, GraphDefinition};

#[cfg(all(feature = "compiler", test))]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "compiler",
    derive(serde_derive::Serialize, serde_derive::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum FlowStatus {
    Initialized,
    Updating,
    Transitioning,
    Interrupted,
    Deactivated,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    #[default]
    Exited,
    Entering,
    Entered,
    Exiting,
}

impl FromStr for FlowState {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "exited" => Ok(FlowState::Exited),
            "entering" => Ok(FlowState::Entering),
            "entered" => Ok(FlowState::Entered),
            "exiting" => Ok(FlowState::Exiting),
            _ => Err(()),
        }
    }
}

impl FlowState {
    pub fn reset(&mut self) {
        *self = FlowState::Exited;
    }

    pub fn is_active(&self) -> bool {
        *self != FlowState::Exited
    }
}

#[derive(Debug, Clone)]
pub struct GraphTime {
    pub delta_time: Seconds,
    pub time_scale: f32,
}

impl Default for GraphTime {
    fn default() -> Self {
        Self {
            delta_time: Default::default(),
            time_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterpreterContext {
    pub machine: MachineIndex,
    pub state: StateIndex,
    pub layer_weight: LayerWeight,
    pub transition_weight: Alpha,
    pub time: GraphTime,
}

impl InterpreterContext {
    fn set_initial_delta_time(&mut self, delta_time: Seconds) {
        self.time.delta_time = delta_time;
    }

    pub fn delta_time(&self) -> Seconds {
        self.time.delta_time
    }

    pub fn apply_time_scale(&mut self, factor: f32) {
        self.time.delta_time *= factor;
        self.time.time_scale *= factor;
    }
}

impl InterpreterContext {
    pub fn new_active() -> Self {
        Self {
            machine: Default::default(),
            state: Default::default(),
            layer_weight: LayerWeight::ONE,
            transition_weight: ALPHA_ONE,
            time: Default::default(),
        }
    }

    pub const fn new_inactive() -> Self {
        Self {
            machine: MachineIndex(IndexType::MAX),
            state: StateIndex(IndexType::MAX),
            layer_weight: LayerWeight::ZERO,
            transition_weight: ALPHA_ZERO,
            time: GraphTime {
                delta_time: Seconds(0.0),
                time_scale: 0.0,
            },
        }
    }
}

impl Default for InterpreterContext {
    fn default() -> Self {
        Self {
            machine: Default::default(),
            state: Default::default(),
            layer_weight: LayerWeight::ZERO,
            transition_weight: ALPHA_ZERO,
            time: Default::default(),
        }
    }
}

pub struct Interpreter<'a> {
    pub visitor: GraphVisitor<'a>,
    pub layers: &'a mut LayerBuilder,
    pub context: InterpreterContext,
}

impl<'a> Interpreter<'a> {
    fn new_internal(
        graph: &'a mut Graph,
        definition: &'a GraphDefinition,
        events: &'a mut FlowEvents,
        layers: &'a mut LayerBuilder,
    ) -> Self {
        Self {
            visitor: GraphVisitor::new(
                graph,
                definition,
                events,
                InterpreterContext::new_active(),
                FlowStatus::Initialized,
            ),
            layers,
            context: InterpreterContext::new_active(),
        }
    }

    pub fn run(
        graph: &'a mut Graph,
        definition: &'a GraphDefinition,
        events: &'a mut FlowEvents,
        layers: &'a mut LayerBuilder,
        dt: f64,
    ) -> Self {
        layers.clear();
        graph.tick();

        let mut runner = Self::new_internal(graph, definition, events, layers);
        runner.context.set_initial_delta_time(Seconds(dt as _));
        runner.eval_machine(MachineIndex(0), None, FlowStatus::Updating);

        runner
    }
}

impl<'a> Interpreter<'a> {
    fn continue_branch(
        &mut self,
        branch_target: IndexType,
        ip: IndexType,
    ) -> (IndexType, HierarchicalBranch) {
        assert!(branch_target > ip, "Backward referencing branch");
        let branch = self.visitor.definition.desc().branches[branch_target as usize];
        (branch_target, branch)
    }

    fn continue_state(&mut self) -> (IndexType, HierarchicalBranch) {
        let branch_target =
            self.visitor.definition.desc().state_branch[self.context.state.0 as usize];
        let branch = self.visitor.definition.desc().branches[branch_target as usize];
        (branch_target, branch)
    }

    fn initialize_state_branch(&mut self, entered_state: FlowState) {
        self.visitor.graph.set_machine_state(
            self.context.machine,
            self.context.state,
            entered_state,
        );
        let next = self.continue_state();
        self.run_branch(next, FlowStatus::Initialized);
    }

    fn set_node_status(&mut self, node_index: NodeIndex, status: FlowStatus) {
        self.visitor.status = status;
        self.visitor.context = self.context.clone();
        self.visitor.context.time.delta_time = Seconds(0.0);
        self.visitor.visit_node(node_index);
    }

    fn update_node(&mut self, node_index: NodeIndex, status: FlowStatus) {
        self.visitor.status = status;
        self.visitor.context = self.context.clone();
        self.visitor.visit_node(node_index);
        self.context.time = self.visitor.context.time.clone();
        self.context.layer_weight = self.visitor.context.layer_weight;
    }

    fn run_branch(&mut self, (ip, branch): (IndexType, HierarchicalBranch), status: FlowStatus) {
        match branch.target {
            HierarchicalBranchTarget::Endpoint => {
                if let Some(node_index) = branch.node {
                    self.update_node(node_index, status);
                }
                let _ = self
                    .layers
                    .push_layer(&self.context, LayerType::Endpoint, branch.node);
            }
            HierarchicalBranchTarget::PostProcess(branch_index) => {
                if let Some(node_index) = branch.node {
                    self.update_node(node_index, status);
                }
                let next = self.continue_branch(branch_index, ip);
                self.run_branch(next, status);
            }
            HierarchicalBranchTarget::Layers { start, count } => {
                let cookie = self
                    .layers
                    .push_layer(&self.context, LayerType::List, branch.node);

                self.context.layer_weight = LayerWeight::ONE;
                let layer_context = self.context.clone();
                for branch_index in start..start + count as IndexType {
                    let next = self.continue_branch(branch_index, ip);
                    self.run_branch(next, status);
                    self.context = layer_context.clone();
                }

                if let Some(node_index) = branch.node {
                    self.update_node(node_index, status);
                }
                self.layers.pop_layer(&self.context, cookie);
            }
            HierarchicalBranchTarget::Machine(target_machine) => {
                assert!(
                    target_machine > self.context.machine,
                    "Backward referencing machine"
                );

                let parent_context = self.context.clone();

                let cookie = self.eval_machine(target_machine, branch.node, status);

                assert!(self.context.machine == target_machine);

                self.context.transition_weight = parent_context.transition_weight;
                if let Some(node_index) = branch.node {
                    self.update_node(node_index, status);
                }

                self.layers.pop_layer(&self.context, cookie);
                self.context = parent_context;
            }
        }
    }

    fn eval_machine(
        &mut self,
        machine: MachineIndex,
        node: Option<NodeIndex>,
        status: FlowStatus,
    ) -> usize {
        self.context.machine = machine;
        match self.visitor.graph.get_machine_state(machine) {
            Some(state) => {
                self.context.state = state;
            }
            None => {
                let initial_state = self.visitor.graph.eval_initial_state(machine);

                self.context.state = initial_state;
                let cookie = self
                    .layers
                    .push_layer(&self.context, LayerType::StateMachine, node);

                self.context.layer_weight = LayerWeight::ONE;
                self.context.transition_weight = ALPHA_ONE;
                self.initialize_state_branch(FlowState::Entered);
                return cookie;
            }
        };

        let cookie = self
            .layers
            .push_layer(&self.context, LayerType::StateMachine, node);

        if let Some(target_completed) = self.visitor.graph.get_completed_transition(&self.context) {
            let transition_context = self.context.clone();
            self.context.state = self
                .visitor
                .graph
                .get_first_transitioning_state(machine)
                .expect("Transitioning state");
            while self.context.state != target_completed {
                self.notify_state(FlowStatus::Deactivated);
                self.context.state = self.visitor.graph.shutdown_transition(&self.context);
            }
            self.context = transition_context;

            if target_completed == self.context.state {
                self.visitor
                    .graph
                    .set_state(target_completed, FlowState::Entered);
            }
        }

        let mut transitioning_state = self.visitor.graph.get_first_transitioning_state(machine);

        if transitioning_state.is_some() {
            let transition_context = self.context.clone();
            let mut layer_weight = LayerWeight::ZERO;
            while let Some(state) = transitioning_state.take() {
                let state_transition = self
                    .visitor
                    .graph
                    .get_state_transition(state)
                    .expect("Valid state");

                let transition_weight: Alpha;
                let next;
                let node;

                if let Some(transition_index) = state_transition.transition_index {
                    assert!(!state_transition.is_complete());

                    let transition = self
                        .visitor
                        .definition
                        .get_transition(transition_index)
                        .expect("Valid transition");

                    let graph = &*self.visitor.graph;
                    let progress = transition.progress.clone().map(|x| x.get(graph));
                    let duration = transition
                        .duration
                        .clone()
                        .map(|x| x.get(graph))
                        .unwrap_or(Seconds(0.0));
                    let transition_blend = transition.blend.clone().map(|x| x.get(graph));

                    let state_transition = self
                        .visitor
                        .graph
                        .get_state_transition_mut(state)
                        .expect("Valid state");
                    state_transition.duration = duration;
                    if let Some(progress) = progress {
                        state_transition.completion = progress;
                    } else {
                        state_transition.update(self.context.delta_time());
                    }

                    state_transition.blend =
                        transition_blend.unwrap_or(state_transition.completion);

                    transition_weight = state_transition.blend;
                    next = state_transition.next;
                    node = state_transition.node;
                } else {
                    transition_weight = state_transition.blend;
                    next = state_transition.next;
                    node = state_transition.node;
                }

                self.context = InterpreterContext {
                    machine,
                    state,
                    layer_weight: LayerWeight::ONE,
                    transition_weight,
                    time: transition_context.time.clone(),
                };

                let branch_status = if next.is_some() {
                    if let Some(node_index) = node {
                        self.update_node(node_index, FlowStatus::Transitioning);
                    }
                    FlowStatus::Transitioning
                } else {
                    assert!(node.is_none());
                    status
                };

                let next_branch = self.continue_state();
                self.run_branch(next_branch, branch_status);

                layer_weight = LayerWeight::interpolate(
                    layer_weight,
                    self.context.layer_weight,
                    transition_weight,
                );

                transitioning_state = next;
            }

            self.context = transition_context;
            self.context.layer_weight = layer_weight;
        } else {
            self.context.layer_weight = LayerWeight::ONE;
            self.context.transition_weight = ALPHA_ONE;
            let next = self.continue_state();
            self.run_branch(next, status);
        }

        if status == FlowStatus::Transitioning {
            return cookie;
        }

        let transitions = self
            .visitor
            .definition
            .get_state_transitions(self.context.state)
            .expect("Valid state index");
        for transition_index in transitions {
            let transition = self
                .visitor
                .definition
                .get_transition(transition_index)
                .expect("Valid transition index");
            let target_is_active = self.visitor.graph.is_state_active(transition.target);
            let condition = self
                .visitor
                .definition
                .get_transition_condition(transition_index)
                .expect("Valid transition condition");

            let result = if transition.is_forced_transition_allowed() || !target_is_active {
                self.visitor.graph.eval_condition(&self.context, &condition)
            } else {
                false
            };

            if !result {
                continue;
            }

            if transition.is_immediate() || target_is_active {
                // Shutdown everything
                self.layers.reset_layer_children(&self.context);

                let branch_status = if target_is_active {
                    FlowStatus::Interrupted
                } else {
                    FlowStatus::Deactivated
                };

                while let Some(state) = self.visitor.graph.get_first_transitioning_state(machine) {
                    self.context.state = state;
                    self.notify_state(branch_status);
                    self.context.state = self.visitor.graph.shutdown_transition(&self.context);
                }

                self.notify_state(branch_status);
            } else {
                self.notify_state(FlowStatus::Transitioning);
            }

            if let Some(node_index) = transition.node {
                self.update_node(node_index, FlowStatus::Initialized);
            }

            let branch_status = if transition.is_immediate() {
                if let Some(node_index) = transition.node {
                    self.set_node_status(node_index, FlowStatus::Deactivated);
                }
                self.context.transition_weight = ALPHA_ONE;
                FlowState::Entered
            } else {
                // NOTE: Currently the origin state is inactive when a forced transition occurs
                self.context.transition_weight =
                    self.visitor
                        .graph
                        .init_transition(&self.context, transition, transition_index);
                FlowState::Entering
            };

            self.context.state = transition.target;

            let source_layer_weight = self.context.layer_weight;
            let transition_weight = self.context.transition_weight;

            self.initialize_state_branch(branch_status);

            self.context.layer_weight = LayerWeight::interpolate(
                source_layer_weight,
                self.context.layer_weight,
                transition_weight,
            );

            break;
        }

        return cookie;
    }

    fn notify_flow_status(
        &mut self,
        (ip, branch): (IndexType, HierarchicalBranch),
        status: FlowStatus,
    ) {
        match branch.target {
            HierarchicalBranchTarget::Endpoint => {}
            HierarchicalBranchTarget::PostProcess(branch_index) => {
                if let Some(node_index) = branch.node {
                    self.set_node_status(node_index, status);
                }
                let next = self.continue_branch(branch_index, ip);
                return self.notify_flow_status(next, status);
            }
            HierarchicalBranchTarget::Layers { start, count } => {
                let layer_context = self.context.clone();
                for branch_index in start..start + count as IndexType {
                    self.context.layer_weight = LayerWeight::ONE;
                    let next = self.continue_branch(branch_index, ip);
                    self.notify_flow_status(next, status);
                    self.context = layer_context.clone();
                }
            }
            HierarchicalBranchTarget::Machine(machine) => {
                assert!(
                    machine > self.context.machine,
                    "Backward referencing machine"
                );

                let parent_context = self.context.clone();
                let active_state = self
                    .visitor
                    .graph
                    .get_machine_state(machine)
                    .expect("Active state");

                self.context.machine = machine;
                self.context.state = active_state;
                if status == FlowStatus::Deactivated || status == FlowStatus::Interrupted {
                    while let Some(state) =
                        self.visitor.graph.get_first_transitioning_state(machine)
                    {
                        self.context.state = state;
                        self.notify_state(status);
                        self.context.state = self.visitor.graph.shutdown_transition(&self.context);
                    }
                }

                if status == FlowStatus::Deactivated || status == FlowStatus::Interrupted {
                    self.visitor
                        .graph
                        .set_state(self.context.state, FlowState::Exited);
                } else if status == FlowStatus::Transitioning {
                    self.visitor
                        .graph
                        .set_state(self.context.state, FlowState::Exiting);
                }

                let next = self.continue_state();
                self.notify_flow_status(next, status);

                if status == FlowStatus::Deactivated || status == FlowStatus::Interrupted {
                    self.visitor.graph.reset_machine_state(self.context.machine);
                }

                self.context = parent_context;
            }
        }

        if let Some(node_index) = branch.node {
            self.set_node_status(node_index, status);
        }
    }

    fn notify_state(&mut self, status: FlowStatus) {
        let transition_context = self.context.clone();
        if status == FlowStatus::Transitioning {
            self.visitor
                .graph
                .set_state(self.context.state, FlowState::Exiting);
        } else if status == FlowStatus::Interrupted || status == FlowStatus::Deactivated {
            self.visitor
                .graph
                .set_state(self.context.state, FlowState::Exited);

            if let Some(node_index) = self
                .visitor
                .graph
                .get_state_transition(self.context.state)
                .expect("Valid state")
                .node
            {
                self.set_node_status(node_index, status);
            }
        }

        let branch = self.continue_state();
        self.notify_flow_status(branch, status);
        self.context = transition_context;
    }
}
