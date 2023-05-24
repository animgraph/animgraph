use std::{
    cmp::Ordering, collections::HashMap, fmt::Debug, marker::PhantomData, ops::Range, sync::Arc,
};

use anyhow::Context;
use serde::de::DeserializeOwned;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    data::ResourceSettings,
    io::{BoolMut, Event, NumberMut, NumberRef, Resource, Timer, VectorMut, VectorRef},
    processors::{GraphNode, GraphVisitor},
    state_machine::{
        ConstantIndex, HierarchicalBranch, HierarchicalStateMachine, HierarchicalTransition,
        MachineIndex, NodeIndex, StateIndex, StateMachineValidationError, VariableIndex,
    },
    FromFloatUnchecked, Graph, GraphBoolean, GraphNumber, Id, IndexType, PoseNode, PoseParent,
    Skeleton, EMPTY_SKELETON,
};

pub enum GraphNodeEntry {
    Process(Box<dyn GraphNode>),
    Pose(Box<dyn PoseNode>),
    PoseParent(Box<dyn PoseParent>),
}

impl GraphNodeEntry {
    pub fn visit_node(&self, visitor: &mut GraphVisitor) {
        match self {
            GraphNodeEntry::Process(node) => node.visit(visitor),
            GraphNodeEntry::Pose(node) => node.visit(visitor),
            GraphNodeEntry::PoseParent(node) => node.visit(visitor),
        }
    }

    pub fn as_pose(&self) -> Option<&dyn PoseNode> {
        match self {
            GraphNodeEntry::Process(_) => None,
            GraphNodeEntry::PoseParent(parent) => parent.as_pose(),
            GraphNodeEntry::Pose(pose) => Some(pose.as_ref()),
        }
    }

    pub fn as_pose_parent(&self) -> Option<&dyn PoseParent> {
        match self {
            GraphNodeEntry::PoseParent(parent) => Some(parent.as_ref()),
            _ => None,
        }
    }
}

// TODO: FIXME
pub type BooleanVec = Vec<bool>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GraphResourceEntry {
    pub resource_type: IndexType,
    pub initial: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphParameterEntry {
    Boolean(IndexType, bool),
    Number(IndexType, f64),
    Vector(IndexType, [f64; 3]),
    Timer(IndexType),
    Event(IndexType),
}

impl GraphParameterEntry {
    fn as_bool(&self) -> Option<BoolMut> {
        match self {
            GraphParameterEntry::Boolean(index, ..) => Some(BoolMut::new(VariableIndex(*index))),
            _ => None,
        }
    }

    fn as_vector(&self) -> Option<VectorMut> {
        match self {
            GraphParameterEntry::Vector(index, ..) => Some(VectorMut(VariableIndex(*index))),
            _ => None,
        }
    }

    fn as_number<T: FromFloatUnchecked>(&self) -> Option<NumberMut<T>> {
        match self {
            GraphParameterEntry::Number(index, ..) => Some(NumberMut::new(VariableIndex(*index))),
            _ => None,
        }
    }

    fn as_timer(&self) -> Option<Timer> {
        match self {
            GraphParameterEntry::Timer(index) => Some(Timer(*index)),
            _ => None,
        }
    }

    fn as_event(&self) -> Option<Event> {
        match self {
            GraphParameterEntry::Event(index) => Some(Event(*index)),
            _ => None,
        }
    }

    fn validate(&self, name: &str, metrics: &GraphMetrics) -> Result<(), GraphReferenceError> {
        match self {
            GraphParameterEntry::Boolean(index, _) => {
                if *index as usize >= metrics.bools {
                    return Err(GraphReferenceError::InvalidBool(name.to_owned()));
                }
            }
            GraphParameterEntry::Number(index, _) => {
                if *index as usize >= metrics.numbers {
                    return Err(GraphReferenceError::InvalidBool(name.to_owned()));
                }
            }
            GraphParameterEntry::Vector(index, _) => {
                if *index as usize + 3 > metrics.numbers {
                    return Err(GraphReferenceError::InvalidBool(name.to_owned()));
                }
            }
            GraphParameterEntry::Timer(index) => {
                if *index as usize >= metrics.timers {
                    return Err(GraphReferenceError::InvalidBool(name.to_owned()));
                }
            }
            GraphParameterEntry::Event(index) => {
                if *index as usize >= metrics.events {
                    return Err(GraphReferenceError::InvalidBool(name.to_owned()));
                }
            }
        }
        Ok(())
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct GraphCompiledNode {
    pub type_id: IndexType,
    pub value: Value,
}

pub trait GraphNodeProvider {
    fn create_nodes(
        &self,
        node_types: Vec<String>,
        compiled_nodes: Vec<GraphCompiledNode>,
        metrics: &mut GraphMetrics,
    ) -> anyhow::Result<Vec<GraphNodeEntry>>;
}

pub trait GraphNodeBuilder {
    fn create(
        &self,
        name: &str,
        node: GraphCompiledNode,
        metrics: &GraphMetrics,
    ) -> anyhow::Result<GraphNodeEntry>;
}

pub trait GraphNodeConstructor: DeserializeOwned {
    fn identity() -> &'static str;
    fn construct_entry(self, metrics: &GraphMetrics) -> anyhow::Result<GraphNodeEntry>;
}

pub struct GraphNodeConstructorMarker<'a, T: GraphNodeConstructor + 'a>(PhantomData<&'a T>);

impl<'a, T: GraphNodeConstructor + 'a> GraphNodeBuilder for GraphNodeConstructorMarker<'a, T> {
    fn create(
        &self,
        name: &str,
        node: GraphCompiledNode,
        metrics: &GraphMetrics,
    ) -> anyhow::Result<GraphNodeEntry> {
        assert_eq!(name, T::identity());
        let value: T = serde_json::from_value(node.value)?;
        value.construct_entry(metrics)
    }
}

#[derive(Default, Clone)]
pub struct GraphNodeRegistry {
    pub builders: HashMap<String, Arc<dyn GraphNodeBuilder>>,
}

impl GraphNodeRegistry {
    pub fn add(&mut self, name: &str, provider: Arc<dyn GraphNodeBuilder>) {
        self.builders.insert(name.to_owned(), provider);
    }

    pub fn register<T: GraphNodeConstructor + 'static>(&mut self) {
        self.add(
            T::identity(),
            Arc::new(GraphNodeConstructorMarker::<T>(PhantomData)),
        )
    }
}

impl GraphNodeProvider for GraphNodeRegistry {
    fn create_nodes(
        &self,
        node_types: Vec<String>,
        compiled_nodes: Vec<GraphCompiledNode>,
        metrics: &mut GraphMetrics,
    ) -> anyhow::Result<Vec<GraphNodeEntry>> {
        let mut providers = Vec::with_capacity(node_types.len());
        for id in node_types.iter() {
            providers.push(
                self.builders
                    .get(id)
                    .ok_or_else(|| GraphBuilderError::MissingNodeBuilder(id.clone()))?,
            );
        }

        let mut nodes = Vec::with_capacity(compiled_nodes.len());

        for node in compiled_nodes {
            let provider = providers.get(node.type_id as usize).ok_or_else(|| {
                GraphBuilderError::MissingNodeBuilder(format!(
                    "id: {} value: {:?}",
                    node.type_id, node.value
                ))
            })?;

            let name = &node_types[node.type_id as usize];
            metrics.current_node = NodeIndex(nodes.len() as _);
            nodes.push(
                provider.create(name, node, metrics).map_err(|err| {
                    GraphBuilderError::NodeInitializationFailed(name.clone(), err)
                })?,
            );
        }

        Ok(nodes)
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct GraphDefinitionBuilder {
    pub parameters: HashMap<String, GraphParameterEntry>,
    pub desc: HierarchicalStateMachine,
    pub compiled_nodes: Vec<GraphCompiledNode>,
    pub node_types: Vec<String>,
    pub resources: Vec<GraphResourceEntry>,
    pub resource_types: Vec<String>,
    pub constants: Vec<f64>,
    pub events: Vec<Id>,
    pub booleans: IndexType,
    pub numbers: IndexType,
    pub timers: IndexType,
}

impl GraphDefinitionBuilder {
    pub fn metrics(&self) -> GraphMetrics {
        GraphMetrics {
            nodes: self.compiled_nodes.len(),
            constants: self.constants.len(),
            numbers: self.numbers as usize,
            bools: self.booleans as usize,
            events: self.events.len(),
            machines: self.desc.total_machines(),
            states: self.desc.total_states(),
            timers: self.timers as usize,
            subconditions: self.desc.total_subconditions(),
            expressions: self.desc.total_expressions(),
            resource_variables: self.resources.iter().map(|x| x.resource_type).collect(),
            resource_types: self.resource_types.clone(),
            current_node: NodeIndex(0),
        }
    }
    pub fn build(
        self,
        registry: &dyn GraphNodeProvider,
    ) -> Result<Arc<GraphDefinition>, GraphBuilderError> {
        let mut metrics = self.metrics();

        self.desc.validate(&metrics)?;

        let GraphDefinitionBuilder {
            parameters,
            node_types,
            compiled_nodes,
            desc,
            constants,
            resources,
            resource_types,
            events,
            booleans,
            numbers,
            timers,
        } = self;

        for (name, param) in parameters.iter() {
            if let Err(err) = param.validate(name, &metrics) {
                return Err(GraphBuilderError::ParameterReferenceError(err));
            }
        }

        let nodes = registry.create_nodes(node_types, compiled_nodes, &mut metrics)?;

        for resource in resources.iter() {
            if resource.resource_type as usize >= resource_types.len() {
                return Err(GraphBuilderError::ResourceTypeIndexOutOfRange);
            }
        }

        Ok(Arc::new(GraphDefinition {
            parameters,
            desc,
            nodes,
            constants,
            resources,
            resource_types,
            events,
            booleans,
            numbers,
            timers,
        }))
    }
}

pub struct GraphDefinition {
    parameters: HashMap<String, GraphParameterEntry>,
    desc: HierarchicalStateMachine,
    nodes: Vec<GraphNodeEntry>,
    constants: Vec<f64>,
    events: Vec<Id>,

    resources: Vec<GraphResourceEntry>,
    resource_types: Vec<String>,

    booleans: IndexType,
    numbers: IndexType,
    timers: IndexType,
}

impl GraphDefinition {
    pub fn get_constant_number(&self, index: ConstantIndex) -> f64 {
        *self.constants.get(index.0 as usize).unwrap_or(&0.0)
    }

    pub fn get_nodes(&self, range: Range<IndexType>) -> Option<&[GraphNodeEntry]> {
        let index = range.start as usize..range.end as usize;
        self.nodes.get(index)
    }

    pub fn get_node(&self, node_index: NodeIndex) -> Option<&GraphNodeEntry> {
        self.nodes.get(node_index.0 as usize)
    }

    pub fn get_bool_parameter(&self, name: &str) -> Option<BoolMut> {
        self.parameters.get(name).and_then(|x| x.as_bool())
    }

    pub fn get_number_parameter<T: FromFloatUnchecked>(&self, name: &str) -> Option<NumberMut<T>> {
        self.parameters.get(name).and_then(|x| x.as_number())
    }

    pub fn get_vector_parameter(&self, name: &str) -> Option<VectorMut> {
        self.parameters.get(name).and_then(|x| x.as_vector())
    }

    pub fn get_timer_parameter(&self, name: &str) -> Option<Timer> {
        self.parameters.get(name).and_then(|x| x.as_timer())
    }

    pub fn get_event_parameter(&self, name: &str) -> Option<Event> {
        self.parameters.get(name).and_then(|x| x.as_event())
    }

    pub fn get_event_by_id(&self, id: Id) -> Option<Event> {
        self.events
            .iter()
            .position(|x| *x == id)
            .map(|x| Event(x as _))
    }

    pub fn get_event_by_name(&self, name: &str) -> Option<Event> {
        self.get_event_by_id(Id::from_str(name))
    }

    pub fn resources_entries(&self) -> &[GraphResourceEntry] {
        &self.resources
    }

    pub fn resource_types(&self) -> &[String] {
        &self.resource_types
    }

    pub fn reset_parameters(&self, graph: &mut Graph) {
        for (_name, parameter) in self.parameters.iter() {
            match parameter {
                &GraphParameterEntry::Boolean(index, value) => {
                    graph.set_variable_boolean(VariableIndex(index), value);
                }
                &GraphParameterEntry::Number(index, value) => {
                    graph.set_variable_number(VariableIndex(index), value as f32);
                }
                &GraphParameterEntry::Vector(index, values) => {
                    let values: [f32; 3] = std::array::from_fn(|i| values[i] as f32);
                    graph.set_variable_number_array(VariableIndex(index), values);
                }
                GraphParameterEntry::Timer(..) | GraphParameterEntry::Event(..) => {}
            }
        }
    }

    pub fn build(
        self: Arc<GraphDefinition>,
        provider: Arc<dyn GraphResourceProvider>,
        skeleton: Arc<Skeleton>,
    ) -> Graph {
        let mut resources = vec![GraphResourceRef(u32::MAX); self.resources.len()];
        provider.initialize(&mut resources, &self);
        let mut graph = Graph::new_internal(self, resources, provider, skeleton);

        let definition = graph.definition().clone();
        definition.reset_parameters(&mut graph);
        graph
    }

    pub fn build_with_empty_skeleton(
        self: Arc<GraphDefinition>,
        provider: Arc<dyn GraphResourceProvider>,
    ) -> Graph {
        self.build(provider, Arc::new(EMPTY_SKELETON.clone()))
    }
}

impl GraphDefinition {
    pub fn desc(&self) -> &HierarchicalStateMachine {
        &self.desc
    }

    #[cfg(test)]
    pub fn desc_mut(&mut self) -> &mut HierarchicalStateMachine {
        &mut self.desc
    }

    pub fn total_events(&self) -> usize {
        self.events.len()
    }

    pub fn total_timers(&self) -> usize {
        self.timers as usize
    }

    pub fn total_boolean_variables(&self) -> usize {
        self.booleans as usize
    }

    pub fn total_number_variables(&self) -> usize {
        self.numbers as usize
    }

    pub fn get_event_id(&self, index: Event) -> Id {
        *self.events.get(index.0 as usize).unwrap_or(&Id::EMPTY)
    }

    pub fn get_machine_states(&self, machine: MachineIndex) -> Option<Range<IndexType>> {
        self.desc.states(machine)
    }

    pub fn get_branch(&self, branch_target: IndexType) -> Option<HierarchicalBranch> {
        self.desc().branch(branch_target)
    }

    pub fn get_state_branch(
        &self,
        state_index: StateIndex,
    ) -> Option<(IndexType, HierarchicalBranch)> {
        self.desc().state_branch(state_index)
    }

    pub fn get_state_global_condition(&self, state_index: StateIndex) -> Option<GraphCondition> {
        self.desc.state_global_condition(state_index)
    }

    pub fn get_transition_condition(&self, transition: IndexType) -> Option<GraphCondition> {
        self.desc().transition_condition(transition)
    }

    pub fn get_transition(&self, transition: IndexType) -> Option<&HierarchicalTransition> {
        self.desc().transition(transition)
    }

    pub fn get_state_transitions(&self, state_index: StateIndex) -> Option<Range<IndexType>> {
        self.desc().state_transitions(state_index)
    }

    pub fn max_subconditions(&self) -> usize {
        self.desc.total_subconditions()
    }

    pub fn get_subcondition(&self, index: IndexType) -> Option<GraphCondition> {
        self.desc.subcondition(index)
    }
}

#[derive(Error, Debug)]
pub enum GraphBuilderError {
    #[error("expected {0} nodes got {1}")]
    ExpectedNodes(usize, usize),
    #[error("expected {0} locals got {1}")]
    ExpectedConstants(usize, usize),
    #[error("expected {0} transitions got {1}")]
    ExpectedTransitions(usize, usize),
    #[error("expected {0} resources got {1}")]
    ExpectedResources(usize, usize),
    #[error("initialization failed: {0:?}")]
    InitializationFailed(#[from] anyhow::Error),
    #[error("node initialiation failed: {0:?}")]
    NodeInitializationFailed(String, anyhow::Error),
    #[error("missing node builder: {0:?}")]
    MissingNodeBuilder(String),
    #[error("state machine validation error: {0:?}")]
    StateMachineValidationFailed(#[from] StateMachineValidationError),
    #[error("parameter validation error: {0:?}")]
    ParameterReferenceError(GraphReferenceError),
    #[error("resource type index out of range")]
    ResourceTypeIndexOutOfRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberRange {
    Exclusive(GraphNumber, GraphNumber),
    Inclusive(GraphNumber, GraphNumber),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionExpression {
    #[default]
    Never,
    Always,
    UnaryTrue(GraphBoolean),
    UnaryFalse(GraphBoolean),
    Equal(GraphBoolean, GraphBoolean),
    NotEqual(GraphBoolean, GraphBoolean),

    Like(GraphNumber, GraphNumber),
    NotLike(GraphNumber, GraphNumber),
    Contains(NumberRange, GraphNumber),
    NotContains(NumberRange, GraphNumber),
    Ordering(
        #[serde(with = "OrderingDef")] Ordering,
        GraphNumber,
        GraphNumber,
    ),
    NotOrdering(
        #[serde(with = "OrderingDef")] Ordering,
        GraphNumber,
        GraphNumber,
    ),

    AllOf(IndexType, IndexType, bool),
    NoneOf(IndexType, IndexType, bool),
    AnyTrue(IndexType, IndexType, bool),
    AnyFalse(IndexType, IndexType, bool),
    ExclusiveOr(IndexType, IndexType, bool),
    ExclusiveNot(IndexType, IndexType, bool),
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NumberOperation {
    Add,
    Subtract,
    Divide,
    Multiply,
    Modulus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNumberExpression {
    Binary(NumberOperation, GraphNumber, GraphNumber),
}



#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
#[serde(remote = "Ordering")]
#[repr(i8)]
enum OrderingDef {
    Less = -1,
    Equal = 0,
    Greater = 1,
}

impl ConditionExpression {
    pub const fn not(self) -> Self {
        use ConditionExpression::*;
        match self {
            Never => Always,
            Always => Never,
            Equal(a, b) => NotEqual(a, b),
            NotEqual(a, b) => Equal(a, b),
            UnaryTrue(a) => UnaryFalse(a),
            UnaryFalse(b) => UnaryTrue(b),
            Like(a, b) => NotLike(a, b),
            NotLike(a, b) => Like(a, b),
            Contains(a, b) => NotContains(a, b),
            NotContains(a, b) => Contains(a, b),
            Ordering(a, b, c) => NotOrdering(a, b, c),
            NotOrdering(a, b, c) => Ordering(a, b, c),
            AllOf(a, b, c) => AllOf(a, b, !c),
            NoneOf(a, b, c) => NoneOf(a, b, !c),
            AnyTrue(a, b, c) => AnyTrue(a, b, !c),
            AnyFalse(a, b, c) => AnyFalse(a, b, !c),
            ExclusiveOr(a, b, c) => ExclusiveOr(a, b, !c),
            ExclusiveNot(a, b, c) => ExclusiveNot(a, b, !c),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphExpression {
    Expression(GraphNumberExpression),
    DebugBreak(GraphNumberExpression),
}

impl GraphExpression {
    pub const fn has_debug_break(&self) -> bool {
        matches!(self, GraphExpression::DebugBreak(..))
    }

    pub fn expression(&self) -> &GraphNumberExpression {
        match self {
            Self::Expression(c) => c,
            Self::DebugBreak(c) => c,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphCondition {
    Expression(ConditionExpression),
    DebugBreak(ConditionExpression),
}


impl GraphCondition {
    pub const fn has_debug_break(&self) -> bool {
        matches!(self, GraphCondition::DebugBreak(..))
    }

    pub fn expression(&self) -> &ConditionExpression {
        match self {
            Self::Expression(c) => c,
            Self::DebugBreak(c) => c,
        }
    }

    #[must_use]
    pub const fn not(self) -> Self {
        match self {
            Self::Expression(c) => Self::Expression(c.not()),
            Self::DebugBreak(c) => Self::DebugBreak(c.not()),
        }
    }

    #[must_use]
    pub const fn debug_break(self) -> Self {
        match self {
            Self::Expression(c) => Self::DebugBreak(c),
            c => c,
        }
    }

    pub const fn new(expression: ConditionExpression) -> Self {
        Self::Expression(expression)
    }

    pub const fn always() -> Self {
        Self::new(ConditionExpression::Always)
    }

    pub const fn never() -> Self {
        Self::new(ConditionExpression::Never)
    }

    pub const fn all_of(range: Range<IndexType>) -> Self {
        Self::new(ConditionExpression::AllOf(range.start, range.end, true))
    }

    pub const fn none_of(range: Range<IndexType>) -> Self {
        Self::new(ConditionExpression::NoneOf(range.start, range.end, true))
    }

    pub const fn exlusive_or(range: Range<IndexType>) -> Self {
        Self::new(ConditionExpression::ExclusiveOr(
            range.start,
            range.end,
            true,
        ))
    }

    pub const fn any_true(range: Range<IndexType>) -> Self {
        Self::new(ConditionExpression::AnyTrue(range.start, range.end, true))
    }

    pub const fn any_false(range: Range<IndexType>) -> Self {
        Self::new(ConditionExpression::AnyFalse(range.start, range.end, true))
    }

    pub const fn like(a: GraphNumber, b: GraphNumber) -> Self {
        Self::new(ConditionExpression::Like(a, b))
    }

    pub const fn contains_exclusive(range: (GraphNumber, GraphNumber), x: GraphNumber) -> Self {
        Self::new(ConditionExpression::Contains(
            NumberRange::Exclusive(range.0, range.1),
            x,
        ))
    }

    pub const fn contains_inclusive(range: (GraphNumber, GraphNumber), x: GraphNumber) -> Self {
        Self::new(ConditionExpression::Contains(
            NumberRange::Inclusive(range.0, range.1),
            x,
        ))
    }

    pub const fn not_like(a: GraphNumber, b: GraphNumber) -> Self {
        Self::new(ConditionExpression::NotLike(a, b))
    }

    pub const fn strictly_less(a: GraphNumber, b: GraphNumber) -> Self {
        Self::new(ConditionExpression::Ordering(Ordering::Less, a, b))
    }

    pub const fn greater_or_equal(a: GraphNumber, b: GraphNumber) -> Self {
        Self::new(ConditionExpression::NotOrdering(Ordering::Less, a, b))
    }

    pub const fn strictly_greater(a: GraphNumber, b: GraphNumber) -> Self {
        Self::new(ConditionExpression::Ordering(Ordering::Greater, a, b))
    }

    pub const fn less_or_equal(a: GraphNumber, b: GraphNumber) -> Self {
        Self::new(ConditionExpression::NotOrdering(Ordering::Greater, a, b))
    }

    pub const fn unary_true(a: GraphBoolean) -> Self {
        Self::new(ConditionExpression::UnaryTrue(a))
    }

    pub const fn unary_false(a: GraphBoolean) -> Self {
        Self::new(ConditionExpression::UnaryFalse(a))
    }

    pub const fn equal(a: GraphBoolean, b: GraphBoolean) -> Self {
        Self::new(ConditionExpression::Equal(a, b))
    }

    pub const fn not_equal(a: GraphBoolean, b: GraphBoolean) -> Self {
        Self::new(ConditionExpression::NotEqual(a, b))
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct GraphResourceRef(pub u32);

pub trait GraphResourceProvider: Send + Sync {
    fn initialize(&self, entries: &mut [GraphResourceRef], definition: &GraphDefinition);
    fn get(&self, index: GraphResourceRef) -> &dyn std::any::Any;
}

#[derive(Debug, Clone)]
pub struct EmptyResourceProvider;

impl GraphResourceProvider for EmptyResourceProvider {
    fn initialize(&self, _entries: &mut [GraphResourceRef], _definition: &GraphDefinition) {}
    fn get(&self, _index: GraphResourceRef) -> &dyn std::any::Any {
        &()
    }
}

#[derive(Debug, Clone)]
pub struct SimpleResourceProvider<T> {
    pub resources: Vec<T>,
    pub entries: Vec<GraphResourceRef>,
}

pub trait ResourceType {
    fn get_resource(&self) -> &dyn std::any::Any;
}

impl<T: ResourceSettings + 'static> ResourceType for T {
    fn get_resource(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T: ResourceType + Send + Sync> GraphResourceProvider for SimpleResourceProvider<T> {
    fn initialize(&self, entries: &mut [GraphResourceRef], _definition: &GraphDefinition) {
        if entries.len() == self.entries.len() {
            entries.copy_from_slice(&self.entries)
        }
    }

    fn get(&self, index: GraphResourceRef) -> &dyn std::any::Any {
        self.resources
            .get(index.0 as usize)
            .map(|x| x.get_resource())
            .unwrap_or(&() as &dyn std::any::Any)
    }
}

#[derive(Error, Debug)]
pub enum SimpleResourceProviderError {
    #[error("resource types does not match")]
    ResourceTypesMismatch(String),
    #[error("deserialization error: {0:?}")]
    DeserializationError(#[from] serde_json::Error),
    #[error("deserialization error: {0:?}")]
    ContextError(#[from] anyhow::Error),
}

impl<T> SimpleResourceProvider<T> {
    pub fn new_with_map(
        definition: &GraphDefinition,
        empty_value: T,
        map: impl Fn(&str, Value) -> anyhow::Result<T>,
    ) -> Result<Self, SimpleResourceProviderError> {
        let resource_entries = definition.resources_entries();
        let mut resources = Vec::new();
        resources.push(empty_value);
        let mut entries: Vec<GraphResourceRef> = Vec::with_capacity(resource_entries.len());
        let types = definition.resource_types();
        const EMPTY_REF: GraphResourceRef = GraphResourceRef(0);

        for entry in resource_entries {
            if let Some(content) = entry.initial.clone() {
                let resource_type = types
                    .get(entry.resource_type as usize)
                    .context("Unknown resources type")?;
                let entry = GraphResourceRef(resources.len() as _);
                resources.push(map(resource_type, content)?);
                entries.push(entry);
            } else {
                entries.push(EMPTY_REF);
            }
        }

        Ok(Self { resources, entries })
    }
}

#[derive(Debug, Clone)]
pub struct GraphMetrics {
    pub nodes: usize,
    pub constants: usize,
    pub numbers: usize,
    pub bools: usize,
    pub events: usize,
    pub timers: usize,
    pub machines: usize,
    pub states: usize,
    pub subconditions: usize,
    pub expressions: usize,
    pub resource_variables: Vec<IndexType>,
    pub resource_types: Vec<String>,
    pub current_node: NodeIndex,
}

#[derive(Error, Debug)]
pub enum GraphReferenceError {
    #[error("invalid constant reference")]
    InvalidConstant(String),
    #[error("invalid number reference")]
    InvalidNumber(String),
    #[error("invalid number expression reference")]
    InvalidNumberExpression(String),
    #[error("invalid vector reference")]
    InvalidVector(String),
    #[error("invalid bool reference")]
    InvalidBool(String),
    #[error("invalid node reference")]
    InvalidNode(String),
    #[error("invalid expression list")]
    InvalidExpressionList(String),
    #[error("missing subexpression")]
    MissingSubExpressions(String),
    #[error("missing machine reference")]
    MissingMachineReference(String),
    #[error("missing state reference")]
    MissingStateReference(String),
    #[error("missing event reference")]
    MissingEventReference(String),
    #[error("missing timer reference")]
    MissingTimerReference(String),
    #[error("missing resource reference")]
    MissingResourceReference(String),
    #[error("invalid resource reference type")]
    InvalidResourceReferenceType(String),
    #[error("back referencing children")]
    BackReferencingChildren(String),
    #[error("missing child references")]
    MissingChildReferences(String),
}

impl GraphMetrics {
    pub fn validate_children(
        &self,
        range: &Range<IndexType>,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        if range.end as usize > self.nodes {
            return Err(GraphReferenceError::MissingChildReferences(
                context.to_owned(),
            ));
        }

        if !range.is_empty() && range.start <= self.current_node.0 {
            return Err(GraphReferenceError::BackReferencingChildren(
                context.to_owned(),
            ));
        }

        Ok(())
    }

    pub fn validate_timer(&self, timer: &Timer, context: &str) -> Result<(), GraphReferenceError> {
        if timer.0 as usize >= self.timers {
            return Err(GraphReferenceError::MissingTimerReference(
                context.to_owned(),
            ));
        }
        Ok(())
    }

    pub fn validate_resource<T: ResourceSettings>(
        &self,
        resource: &Resource<T>,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        match self.resource_variables.get(resource.variable as usize) {
            Some(&resource_type) => {
                if self
                    .resource_types
                    .get(resource_type as usize)
                    .map(|x| x.as_str())
                    != Some(T::resource_type())
                {
                    return Err(GraphReferenceError::InvalidResourceReferenceType(
                        context.to_owned(),
                    ));
                }
            }
            None => {
                return Err(GraphReferenceError::MissingResourceReference(
                    context.to_owned(),
                ));
            }
        }

        Ok(())
    }

    pub fn validate_event(&self, event: &Event, context: &str) -> Result<(), GraphReferenceError> {
        if event.0 as usize >= self.events {
            return Err(GraphReferenceError::MissingEventReference(
                context.to_owned(),
            ));
        }
        Ok(())
    }

    pub fn validate_number(
        &self,
        number: GraphNumber,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        match number {
            GraphNumber::Zero | GraphNumber::One | GraphNumber::Iteration => {}
            GraphNumber::Constant(ConstantIndex(index)) => {
                if index as usize >= self.constants {
                    return Err(GraphReferenceError::InvalidConstant(context.to_owned()));
                }
            }
            GraphNumber::Variable(VariableIndex(index)) => {
                if index as usize >= self.numbers {
                    return Err(GraphReferenceError::InvalidNumber(context.to_owned()));
                }
            }
            GraphNumber::Projection(_, index) => {
                if index.0 as usize + 3 > self.numbers {
                    return Err(GraphReferenceError::InvalidVector(format!(
                        "{:?}: {}",
                        number, context
                    )));
                }
            }
            GraphNumber::Expression(index) => {
                if index as usize >= self.expressions {
                    return Err(GraphReferenceError::InvalidNumberExpression(format!(
                        "{:?}: {}",
                        number, context
                    )));
                }
            },
        }
        Ok(())
    }

    pub fn validate_number_ref<T: FromFloatUnchecked>(
        &self,
        number: &NumberRef<T>,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        self.validate_number(number.variable(), context)
    }

    pub fn validate_number_mut<T: FromFloatUnchecked>(
        &self,
        number: &NumberMut<T>,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        if number.index.0 as usize >= self.numbers {
            return Err(GraphReferenceError::InvalidNumber(context.to_owned()));
        }
        Ok(())
    }

    pub fn validate_bool_mut(
        &self,
        value: &BoolMut,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        if value.index.0 as usize >= self.bools {
            return Err(GraphReferenceError::InvalidBool(context.to_owned()));
        }
        Ok(())
    }

    pub fn validate_bool(
        &self,
        value: GraphBoolean,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        match value {
            GraphBoolean::Never | GraphBoolean::Always => {}
            GraphBoolean::Variable(VariableIndex(index)) => {
                if index as usize >= self.bools {
                    return Err(GraphReferenceError::InvalidBool(context.to_owned()));
                }
            }
            GraphBoolean::Condition(index) => {
                if index as usize >= self.subconditions {
                    return Err(GraphReferenceError::MissingSubExpressions(
                        context.to_owned(),
                    ));
                }
            }
            GraphBoolean::QueryMachineActive(MachineIndex(index))
            | GraphBoolean::QueryMachineState(_, MachineIndex(index)) => {
                if index as usize >= self.machines {
                    return Err(GraphReferenceError::MissingMachineReference(
                        context.to_owned(),
                    ));
                }
            }
            GraphBoolean::QueryStateActive(StateIndex(index))
            | GraphBoolean::QueryState(_, StateIndex(index)) => {
                if index as usize >= self.states {
                    return Err(GraphReferenceError::MissingStateReference(
                        context.to_owned(),
                    ));
                }
            }
            GraphBoolean::QueryEventActive(Event(index))
            | GraphBoolean::QueryEvent(_, Event(index)) => {
                if index as usize >= self.events {
                    return Err(GraphReferenceError::MissingEventReference(
                        context.to_owned(),
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn validate_condition_range(
        &self,
        a: IndexType,
        b: IndexType,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        if a >= b {
            return Err(GraphReferenceError::InvalidExpressionList(
                context.to_owned(),
            ));
        }

        if b as usize > self.subconditions {
            return Err(GraphReferenceError::MissingSubExpressions(
                context.to_owned(),
            ));
        }
        Ok(())
    }

    pub fn validate_node_index(
        &self,
        node: Option<NodeIndex>,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        match node {
            Some(NodeIndex(index)) if (index as usize) >= self.nodes => {
                Err(GraphReferenceError::InvalidNode(context.to_owned()))
            }
            _ => Ok(()),
        }
    }

    pub fn validate_vector_ref(
        &self,
        value: &VectorRef,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        match value {
            VectorRef::Constant(_vec) => {}
            VectorRef::Variable(var) => {
                if var.0 as usize + 3 > self.numbers {
                    return Err(GraphReferenceError::InvalidVector(context.to_owned()));
                }
            }
        }
        Ok(())
    }

    pub fn validate_vector_mut(
        &self,
        value: &VectorMut,
        context: &str,
    ) -> Result<(), GraphReferenceError> {
        if value.0 .0 as usize + 3 > self.numbers {
            return Err(GraphReferenceError::InvalidVector(context.to_owned()));
        }
        Ok(())
    }
}
