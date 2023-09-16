use std::ops::Range;

use serde_derive::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    io::NumberRef, Alpha, ConditionExpression, GraphCondition, GraphExpression, GraphMetrics,
    GraphReferenceError, IndexType, Seconds,
};

pub type StateOffset = IndexType;
pub type TransitionsOffset = IndexType;
pub type BranchIndex = IndexType;

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct ConstantIndex(pub IndexType);

impl From<ConstantIndex> for usize {
    fn from(value: ConstantIndex) -> Self {
        value.0 as usize
    }
}

impl From<IndexType> for ConstantIndex {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct VariableIndex(pub IndexType);

impl From<VariableIndex> for usize {
    fn from(value: VariableIndex) -> Self {
        value.0 as usize
    }
}

impl From<IndexType> for VariableIndex {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct MachineIndex(pub IndexType);

impl From<MachineIndex> for usize {
    fn from(value: MachineIndex) -> Self {
        value.0 as usize
    }
}

impl From<IndexType> for MachineIndex {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct StateIndex(pub IndexType);

impl From<StateIndex> for usize {
    fn from(value: StateIndex) -> Self {
        value.0 as usize
    }
}

impl From<IndexType> for StateIndex {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct NodeIndex(pub IndexType);

impl From<NodeIndex> for usize {
    fn from(value: NodeIndex) -> Self {
        value.0 as usize
    }
}

impl From<IndexType> for NodeIndex {
    fn from(value: IndexType) -> Self {
        Self(value)
    }
}

#[derive(Error, Debug)]
pub enum StateMachineValidationError {
    #[error("too many machines")]
    TooManyMachines,
    #[error("too many states")]
    TooManyStates,
    #[error("too many transitions")]
    TooManyTransitions,
    #[error("too many branches")]
    TooManyBranches,
    #[error("too many conditions")]
    TooManyConditions,

    #[error("corrupt machine states")]
    CorruptMachineStates,
    #[error("corrupt states")]
    CorruptStates,
    #[error("corrupt state transitions")]
    CorruptStateTransitions,
    #[error("missing state branch")]
    MissingStateBranch,
    #[error("corrupt transitions")]
    CorruptTransitions,
    #[error("missing transition origin")]
    MissingTransitionOrigin,
    #[error("missing transition target")]
    MissingTransitionTarget,
    #[error("branch error: target missing")]
    BranchTargetMissing,
    #[error("backwards referencing condition")]
    ConditionBackwardReferencing,
    #[error("backwards referencing expression")]
    ExpressionBackwardReferencing,
    #[error("state branch error: backwards referencing")]
    StateBranchBackwardReferencing,
    #[error("submachine branch error: backwards referencing")]
    SubMachineBranchBackwardReferencing,
    #[error("layer branch error: backwards referencing")]
    LayerBranchBackwardsReferencing,
    #[error("preprocess branch error: backwards referencing")]
    PreprocessBranchBackwardsReferencing,

    #[error("invalid transition origin")]
    InvalidTransitionOrigin,
    #[error("invalid transition origin")]
    InvalidTransitionTarget,

    #[error("{0:?}")]
    GraphReferenceError(#[from] GraphReferenceError),
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalStateMachine {
    pub machine_states: Vec<StateOffset>,
    pub state_transitions: Vec<TransitionsOffset>, // End index of state transitions, see get_child_range
    pub state_branch: Vec<BranchIndex>,
    pub state_global_condition: Vec<GraphCondition>,
    pub transitions: Vec<HierarchicalTransition>,
    pub transition_condition: Vec<GraphCondition>,
    pub branches: Vec<HierarchicalBranch>,

    // move this to GraphDefinitionBuilder?
    pub conditions: Vec<GraphCondition>,
    pub expressions: Vec<GraphExpression>,
}

impl HierarchicalStateMachine {
    fn basic_validation(&self, metrics: &GraphMetrics) -> Result<(), StateMachineValidationError> {
        let HierarchicalStateMachine {
            machine_states,
            state_transitions,
            state_branch,
            state_global_condition,
            transitions,
            transition_condition,
            branches,
            conditions,
            expressions,
        } = self;
        const MAX_COUNT: usize = IndexType::MAX as usize - 1;
        use StateMachineValidationError::*;
        if machine_states.len() >= MAX_COUNT {
            return Err(TooManyMachines);
        }

        match machine_states.first() {
            Some(0) => return Err(CorruptStates),
            Some(_) => {}
            None => return Err(CorruptStates),
        }

        for (a, b) in machine_states.iter().zip(machine_states.iter().skip(1)) {
            // N >= 1
            if *a >= *b {
                return Err(CorruptStates);
            }
        }

        for (a, b) in state_transitions
            .iter()
            .zip(state_transitions.iter().skip(1))
        {
            // N >= 0
            if *a > *b {
                return Err(CorruptStateTransitions);
            }
        }

        for index in state_branch.iter() {
            if *index as usize >= branches.len() {
                return Err(MissingStateBranch);
            }
        }

        let state_count = state_branch.len();
        if state_count >= MAX_COUNT {
            return Err(TooManyStates);
        }

        if state_count != state_transitions.len() || state_count != state_global_condition.len() {
            return Err(CorruptStates);
        }

        if transitions.len() >= MAX_COUNT {
            return Err(TooManyTransitions);
        }

        if transition_condition.len() != transitions.len() {
            return Err(CorruptTransitions);
        }

        for HierarchicalTransition {
            origin,
            target,
            forceable: _,
            immediate: _,
            blend,
            progress,
            duration,
            node,
        } in transitions.iter()
        {
            if origin.0 as usize >= state_count {
                return Err(MissingTransitionOrigin);
            }
            if target.0 as usize >= state_count {
                return Err(MissingTransitionTarget);
            }

            blend.map_or(Ok(()), |x| {
                metrics.validate_number(x.variable(), "transition blend")
            })?;
            progress.map_or(Ok(()), |x| {
                metrics.validate_number(x.variable(), "transition progress")
            })?;
            duration.map_or(Ok(()), |x| {
                metrics.validate_number(x.variable(), "transition duration")
            })?;

            metrics.validate_node_index(*node, "transition node")?;
        }

        if branches.len() >= MAX_COUNT {
            return Err(TooManyBranches);
        }

        for HierarchicalBranch { node, target } in branches.iter() {
            metrics.validate_node_index(*node, "in branch")?;

            match target {
                HierarchicalBranchTarget::Endpoint => {}
                &HierarchicalBranchTarget::PostProcess(index) => {
                    if index as usize >= branches.len() {
                        return Err(BranchTargetMissing);
                    }
                }
                HierarchicalBranchTarget::Machine(index) => {
                    if index.0 as usize >= machine_states.len() {
                        return Err(BranchTargetMissing);
                    }
                }
                &HierarchicalBranchTarget::Layers { start, count } => {
                    if (start as usize + count as usize) > branches.len() {
                        return Err(BranchTargetMissing);
                    }
                }
            }
        }

        if conditions.len() >= MAX_COUNT {
            return Err(TooManyConditions);
        }

        for list in [conditions, state_global_condition, transition_condition] {
            for condition in list {
                match condition.expression().clone() {
                    ConditionExpression::Never | ConditionExpression::Always => {}
                    ConditionExpression::UnaryTrue(value)
                    | ConditionExpression::UnaryFalse(value) => {
                        metrics.validate_bool(value, "in conditional")?
                    }
                    ConditionExpression::Equal(a, b) | ConditionExpression::NotEqual(a, b) => {
                        metrics.validate_bool(a, "in conditional")?;
                        metrics.validate_bool(b, "in conditional")?;
                    }
                    ConditionExpression::Like(a, b)
                    | ConditionExpression::NotLike(a, b)
                    | ConditionExpression::Ordering(_, a, b)
                    | ConditionExpression::NotOrdering(_, a, b) => {
                        metrics.validate_number(a, "in conditional")?;
                        metrics.validate_number(b, "in conditional")?;
                    }

                    ConditionExpression::Contains(range, x)
                    | ConditionExpression::NotContains(range, x) => match range {
                        crate::NumberRange::Exclusive(a, b)
                        | crate::NumberRange::Inclusive(a, b) => {
                            metrics.validate_number(a, "in conditional")?;
                            metrics.validate_number(b, "in conditional")?;
                            metrics.validate_number(x, "in conditional")?;
                        }
                    },
                    ConditionExpression::AllOf(a, b, _)
                    | ConditionExpression::NoneOf(a, b, _)
                    | ConditionExpression::AnyTrue(a, b, _)
                    | ConditionExpression::AnyFalse(a, b, _)
                    | ConditionExpression::ExclusiveOr(a, b, _)
                    | ConditionExpression::ExclusiveNot(a, b, _) => {
                        metrics.validate_condition_range(a, b, "in conditional")?;
                    }
                }
            }
        }

        for (index, condition) in conditions.iter().enumerate() {
            match *condition.expression() {
                ConditionExpression::AllOf(a, _, _)
                | ConditionExpression::NoneOf(a, _, _)
                | ConditionExpression::AnyTrue(a, _, _)
                | ConditionExpression::AnyFalse(a, _, _)
                | ConditionExpression::ExclusiveOr(a, _, _)
                | ConditionExpression::ExclusiveNot(a, _, _) => {
                    if a as usize <= index {
                        return Err(ConditionBackwardReferencing);
                    }
                }
                ConditionExpression::Never
                | ConditionExpression::Always
                | ConditionExpression::UnaryTrue(_)
                | ConditionExpression::UnaryFalse(_)
                | ConditionExpression::Equal(_, _)
                | ConditionExpression::NotEqual(_, _)
                | ConditionExpression::Like(_, _)
                | ConditionExpression::NotLike(_, _)
                | ConditionExpression::Contains(..)
                | ConditionExpression::NotContains(..)
                | ConditionExpression::Ordering(_, _, _)
                | ConditionExpression::NotOrdering(_, _, _) => {}
            }
        }

        for (index, expr) in expressions.iter().enumerate() {
            match *expr.expression() {
                crate::GraphNumberExpression::Binary(_, lhs, rhs) => {
                    match lhs {
                        crate::GraphNumber::Zero => {}
                        crate::GraphNumber::One => {}
                        crate::GraphNumber::Iteration => {}
                        crate::GraphNumber::Projection(_, _) => {}
                        crate::GraphNumber::Constant(_) => {}
                        crate::GraphNumber::Variable(_) => {}
                        crate::GraphNumber::Expression(i) => {
                            if i as usize <= index {
                                return Err(ExpressionBackwardReferencing);
                            }
                        }
                    }

                    match rhs {
                        crate::GraphNumber::Zero => {}
                        crate::GraphNumber::One => {}
                        crate::GraphNumber::Iteration => {}
                        crate::GraphNumber::Projection(_, _) => {}
                        crate::GraphNumber::Constant(_) => {}
                        crate::GraphNumber::Variable(_) => {}
                        crate::GraphNumber::Expression(i) => {
                            if i as usize <= index {
                                return Err(ExpressionBackwardReferencing);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_branches(
        &self,
        state_machine: MachineIndex,
        _state: StateIndex,
        ip: IndexType,
    ) -> Result<(), StateMachineValidationError> {
        let HierarchicalBranch { node: _, target } = &self.branches[ip as usize];

        match *target {
            HierarchicalBranchTarget::Endpoint => {}
            HierarchicalBranchTarget::PostProcess(index) => {
                if index <= ip {
                    return Err(StateMachineValidationError::PreprocessBranchBackwardsReferencing);
                }

                return self.validate_branches(state_machine, _state, index);
            }
            HierarchicalBranchTarget::Machine(submachine) => {
                if submachine.0 <= state_machine.0 {
                    return Err(StateMachineValidationError::SubMachineBranchBackwardReferencing);
                }

                let states = self.states(submachine).expect("Valid states");
                assert!(!states.is_empty(), "States not validated correctly.");
                for state in states {
                    let state_branch = self.state_branch[state as usize];
                    if state_branch <= ip {
                        return Err(StateMachineValidationError::StateBranchBackwardReferencing);
                    }
                    self.validate_branches(submachine, StateIndex(state), state_branch)?;
                }
            }
            HierarchicalBranchTarget::Layers { start, count } => {
                if start <= ip {
                    return Err(StateMachineValidationError::LayerBranchBackwardsReferencing);
                }

                for index in start..start + count as IndexType {
                    self.validate_branches(state_machine, _state, index)?;
                }
            }
        }

        Ok(())
    }

    pub fn validate(&self, metrics: &GraphMetrics) -> Result<(), StateMachineValidationError> {
        self.basic_validation(metrics)?;

        for machine in 0..self.machine_states.len() as IndexType {
            let states = self.states(MachineIndex(machine)).expect("Valid states");
            assert!(!states.is_empty(), "States not validated correctly.");
            for state in states.clone() {
                self.validate_branches(
                    MachineIndex(machine),
                    StateIndex(state),
                    self.state_branch[state as usize],
                )?;

                let transitions = self
                    .state_transitions(StateIndex(state))
                    .expect("Valid state");

                for transition_index in transitions {
                    let HierarchicalTransition {
                        origin,
                        target,
                        forceable: _,
                        immediate: _,
                        blend: _,
                        progress: _,
                        duration: _,
                        node: _,
                    } = &self.transitions[transition_index as usize];

                    if *origin != StateIndex(state) {
                        return Err(StateMachineValidationError::InvalidTransitionOrigin);
                    }

                    if !states.contains(&target.0) {
                        return Err(StateMachineValidationError::InvalidTransitionTarget);
                    }
                }
            }
        }

        Ok(())
    }
}

fn get_child_range(parent: &[IndexType], index: usize) -> Option<Range<IndexType>> {
    let upper = *parent.get(index)?;
    Some(if index == 0 {
        0..upper
    } else {
        let lower = *parent.get(index - 1)?;
        lower..upper
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalReferences {
    pub constants: usize,
    pub variables: usize,
    pub nodes: usize,
}

impl HierarchicalStateMachine {
    pub fn total_machines(&self) -> usize {
        self.machine_states.len()
    }

    pub fn total_states(&self) -> usize {
        self.state_transitions.len()
    }

    pub fn total_transitions(&self) -> usize {
        self.transitions.len()
    }

    pub fn total_subconditions(&self) -> usize {
        self.conditions.len()
    }

    pub fn total_expressions(&self) -> usize {
        self.expressions.len()
    }
    pub fn total_branches(&self) -> usize {
        self.branches.len()
    }

    pub fn states(&self, machine: MachineIndex) -> Option<Range<IndexType>> {
        get_child_range(&self.machine_states, machine.into())
    }

    pub fn state_global_condition(&self, state: StateIndex) -> Option<GraphCondition> {
        self.state_global_condition.get(state.0 as usize).cloned()
    }

    pub fn state_transitions(&self, state_index: StateIndex) -> Option<Range<IndexType>> {
        get_child_range(&self.state_transitions, state_index.0 as usize)
    }

    pub fn transition(&self, transition_index: IndexType) -> Option<&HierarchicalTransition> {
        self.transitions.get(transition_index as usize)
    }

    pub fn state_branch(&self, state_index: StateIndex) -> Option<(IndexType, HierarchicalBranch)> {
        let branch_index = *self.state_branch.get(state_index.0 as usize)?;
        Some((
            branch_index,
            self.branches.get(branch_index as usize).cloned()?,
        ))
    }

    pub fn transition_condition(&self, transition_index: IndexType) -> Option<GraphCondition> {
        self.transition_condition
            .get(transition_index as usize)
            .cloned()
    }

    pub fn subcondition(&self, condition_index: IndexType) -> Option<GraphCondition> {
        self.conditions.get(condition_index as usize).cloned()
    }

    pub fn branch(&self, branch: IndexType) -> Option<HierarchicalBranch> {
        self.branches.get(branch as usize).cloned()
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HierarchicalLayers {
    pub start: IndexType,
    pub count: IndexType,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HierarchicalBranchTarget {
    #[default]
    Endpoint,
    PostProcess(BranchIndex),
    Machine(MachineIndex),
    Layers {
        start: IndexType,
        count: u8,
    },
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HierarchicalBranch {
    pub node: Option<NodeIndex>,
    pub target: HierarchicalBranchTarget,
}

impl HierarchicalBranch {
    pub const ROOT: Self = HierarchicalBranch {
        node: None,
        target: HierarchicalBranchTarget::Machine(MachineIndex(0)),
    };
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchicalTransition {
    pub origin: StateIndex,
    pub target: StateIndex,
    pub forceable: bool,
    pub immediate: bool,
    pub blend: Option<NumberRef<Alpha>>,
    pub progress: Option<NumberRef<Alpha>>,
    pub duration: Option<NumberRef<Seconds>>,
    pub node: Option<NodeIndex>,
}

impl HierarchicalTransition {
    pub const FLAG_IMMEDIATE: IndexType = 1;
    pub const FLAG_CAN_FORCE: IndexType = 2;

    pub fn is_immediate(&self) -> bool {
        self.immediate
    }

    pub fn is_forced_transition_allowed(&self) -> bool {
        self.forceable
    }
}
