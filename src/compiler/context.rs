use std::{collections::HashMap, fmt::Debug, marker::PhantomData, ops::Range, sync::Arc};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Number, Value};
use thiserror::Error;

use crate::{
    io::{BoolMut, Event, NumberMut, NumberRef, Resource, Timer, VectorMut, VectorRef},
    model::{IORouteSettings, NodeChildRange},
    state_machine::{
        BranchIndex, ConstantIndex, HierarchicalBranch, HierarchicalBranchTarget,
        HierarchicalTransition, MachineIndex, NodeIndex, StateIndex, VariableIndex,
    },
    FlowState, FromFloatUnchecked, GraphBoolean, GraphBuilderError, GraphCompiledNode,
    GraphCondition, GraphExpression, GraphMetrics, GraphNodeConstructor, GraphNumber,
    GraphNumberExpression, GraphParameterEntry, GraphResourceEntry, Id, IndexType,
};

use super::{
    prelude::{
        AnimGraph, Branch, BranchTarget, Expression, IOSettings, IOType, InitialParameterValue,
        Node, NodeSettings, Query, QueryType, ResourceSettings, State, StateMachine, Transition,
        DEFAULT_INPUT_NAME, DEFAULT_OUPTUT_NAME, IO,
    },
    GraphDefinitionCompilation,
};

pub trait NodeCompilationMeta: Send + Sync {
    fn name(&self) -> &'static str;
    fn input(&self) -> &'static [(&'static str, IOType)];
    fn output(&self) -> &'static [(&'static str, IOType)];
    fn instantiate_default(&self) -> Node;
    fn child_range(&self) -> NodeChildRange;
    fn build(&self, context: &NodeSerializationContext) -> Result<Value, NodeCompilationError>;
}

#[derive(Error, Debug)]
pub enum NodeCompilationError {
    #[error("missing input slot")]
    MissingInputSlot(String),
    #[error("missing output slot")]
    MissingOutputSlot(String),
    #[error("serialization error for {0}: {1:?}")]
    SettingsSerializationError(String, serde_json::Error),
    #[error("serialization error for {0}: {1:?}")]
    NodeSerializationError(String, serde_json::Error),
    #[error("validation error for {0}: {1:?}")]
    ValidationError(String, anyhow::Error),
}

pub struct NodeSerializationContext<'a> {
    metrics: &'a GraphMetrics,
    node: &'a NodeCompilationContext<'a>,
    resource_types: &'a [String],
}

impl<'a> NodeSerializationContext<'a> {
    pub fn settings<T: NodeSettings + DeserializeOwned>(&self) -> Result<T, NodeCompilationError> {
        serde_json::from_value(self.node.node.settings.clone()).map_err(|err| {
            NodeCompilationError::SettingsSerializationError(self.node.path.join(), err)
        })
    }

    pub fn serialize_node<T: Serialize + GraphNodeConstructor + Clone>(
        &self,
        value: T,
    ) -> Result<Value, NodeCompilationError> {
        if let Err(err) = value.clone().construct_entry(self.metrics) {
            return Err(NodeCompilationError::ValidationError(
                self.node.path.join(),
                err,
            ));
        }

        serde_json::to_value(value)
            .map_err(|err| NodeCompilationError::NodeSerializationError(self.node.path.join(), err))
    }

    pub fn get_input(&self, index: usize) -> Option<&NodeCompilationInput> {
        if !self.node.inputs.is_empty() {
            self.node.inputs.get(index)
        } else if index == 0 {
            self.node.default_input.as_ref()
        } else {
            None
        }
    }

    pub fn get_output(&self, index: usize) -> Option<&NodeCompilationOutput> {
        if !self.node.outputs.is_empty() {
            self.node.outputs.get(index)
        } else if index == 0 {
            self.node.default_output.as_ref()
        } else {
            None
        }
    }

    pub fn input_timer(&self, index: usize) -> Result<Timer, NodeCompilationError> {
        self.get_input(index)
            .and_then(|x| x.as_timer())
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(self.node.path.join()))
    }

    pub fn output_timer(&self, index: usize) -> Result<Timer, NodeCompilationError> {
        self.get_output(index)
            .and_then(|x| x.as_timer())
            .ok_or_else(|| NodeCompilationError::MissingOutputSlot(self.node.path.join()))
    }

    pub fn input_event(&self, index: usize) -> Result<Event, NodeCompilationError> {
        self.get_input(index)
            .and_then(|x| x.as_event())
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(self.node.path.join()))
    }

    pub fn output_event(&self, index: usize) -> Result<Event, NodeCompilationError> {
        self.get_output(index)
            .and_then(|x| x.as_event())
            .ok_or_else(|| NodeCompilationError::MissingOutputSlot(self.node.path.join()))
    }

    pub fn input_bool(&self, index: usize) -> Result<GraphBoolean, NodeCompilationError> {
        self.get_input(index)
            .and_then(|x| x.as_bool())
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(self.node.path.join()))
    }

    pub fn output_bool(&self, index: usize) -> Result<BoolMut, NodeCompilationError> {
        self.get_output(index)
            .and_then(|x| x.as_bool())
            .ok_or_else(|| NodeCompilationError::MissingOutputSlot(self.node.path.join()))
    }

    pub fn input_vector(&self, index: usize) -> Result<VectorRef, NodeCompilationError> {
        self.get_input(index)
            .and_then(|x| x.as_vector())
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(self.node.path.join()))
    }

    pub fn output_vector(&self, index: usize) -> Result<VectorMut, NodeCompilationError> {
        self.get_output(index)
            .and_then(|x| x.as_vector())
            .ok_or_else(|| NodeCompilationError::MissingOutputSlot(self.node.path.join()))
    }

    pub fn children(&self) -> Range<IndexType> {
        self.node
            .children
            .clone()
            .map(|x| x.start.0..x.end.0)
            .unwrap_or(0..0)
    }

    pub fn input_number<T: FromFloatUnchecked>(
        &self,
        index: usize,
    ) -> Result<NumberRef<T>, NodeCompilationError> {
        self.get_input(index)
            .and_then(|x| x.as_number())
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(self.node.path.join()))
    }

    pub fn output_number<T: FromFloatUnchecked>(
        &self,
        index: usize,
    ) -> Result<NumberMut<T>, NodeCompilationError> {
        self.get_output(index)
            .and_then(|x| x.as_number())
            .ok_or_else(|| NodeCompilationError::MissingOutputSlot(self.node.path.join()))
    }

    pub fn input_resource<T: ResourceSettings>(
        &self,
        index: usize,
    ) -> Result<Resource<T>, NodeCompilationError> {
        let resource_type = self
            .resource_types
            .iter()
            .position(|x| x == T::resource_type())
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(dbg!(self.node).path.join()))?;

        self.get_input(index)
            .and_then(|x| x.as_resource::<T>(resource_type))
            .ok_or_else(|| NodeCompilationError::MissingInputSlot(self.node.path.join()))
    }

    pub fn output_resource<T: ResourceSettings>(
        &self,
        index: usize,
    ) -> Result<Resource<T>, NodeCompilationError> {
        self.get_output(index)
            .and_then(|x| x.as_resource::<T>())
            .ok_or_else(|| NodeCompilationError::MissingOutputSlot(self.node.path.join()))
    }
}

pub trait NodeCompiler {
    type Settings: NodeSettings + Default;

    fn build(context: &NodeSerializationContext<'_>) -> Result<Value, NodeCompilationError>;
}

struct NodeCompilerMeta<'a, T: NodeCompiler + 'a>(PhantomData<&'a T>);

impl<'a, T: NodeCompiler + Send + Sync + 'a> NodeCompilationMeta for NodeCompilerMeta<'a, T> {
    fn name(&self) -> &'static str {
        T::Settings::name()
    }

    fn input(&self) -> &'static [(&'static str, IOType)] {
        T::Settings::input()
    }

    fn output(&self) -> &'static [(&'static str, IOType)] {
        T::Settings::output()
    }

    fn instantiate_default(&self) -> Node {
        Node::new_disconnected(T::Settings::default()).expect("Valid")
    }

    fn child_range(&self) -> NodeChildRange {
        T::Settings::child_range()
    }

    fn build(&self, context: &NodeSerializationContext) -> Result<Value, NodeCompilationError> {
        T::build(context)
    }
}

#[derive(Error, Debug)]
pub enum AnimGraphIndexError {
    #[error("maximum resources reached")]
    MaxResourcesReached,
    #[error("maximum resource types reached")]
    MaxResourceTypesReached,
    #[error("maximum events reached")]
    MaxEventsReached,
    #[error("maximum timers reached")]
    MaxTimersReached,
    #[error("maximum booleans reached")]
    MaxBooleansReached,
    #[error("maximum numbers reached")]
    MaxNumbersReached,
    #[error("maximum vectors reached")]
    MaxVectorsReached,
    #[error("maximum constants reached")]
    MaxConstantsReached,
    #[error("maximum states reached")]
    MaxStatesReached,
    #[error("maximum machines reached")]
    MaxStateMachinesReached,
    #[error("maximum transitions reached")]
    MaxTransitionsReached,
    #[error("maximum nodes reached")]
    MaxNodesReached,
    #[error("maximum node types reached")]
    MaxNodeTypesReached,
    #[error("maximum branches reached")]
    MaxBranchesReached,
    #[error("maximum branch children reached")]
    MaxBranchChildrenReached,
    #[error("maximum conditions reached")]
    MaxConditionsReached,
    #[error("maximum expressions reached")]
    MaxExpressionsReached,
}

pub type IndexedResult<T> = Result<T, AnimGraphIndexError>;

pub trait IndexConversions: TryInto<IndexType> {
    fn try_boolean(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxBooleansReached)
    }

    fn try_timer(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxTimersReached)
    }

    fn try_event(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxEventsReached)
    }

    fn try_number(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxNumbersReached)
    }

    fn try_vector(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxVectorsReached)
    }

    fn try_resource_type(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxResourceTypesReached)
    }

    fn try_resource_variable(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxResourcesReached)
    }

    fn try_constant(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxConstantsReached)
    }

    fn try_condition(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxConditionsReached)
    }

    fn try_expression(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxExpressionsReached)
    }

    fn try_state_index(self) -> IndexedResult<StateIndex> {
        Ok(StateIndex(
            self.try_into()
                .map_err(|_| AnimGraphIndexError::MaxStatesReached)?,
        ))
    }

    fn try_machine_index(self) -> IndexedResult<MachineIndex> {
        Ok(MachineIndex(self.try_into().map_err(|_| {
            AnimGraphIndexError::MaxStateMachinesReached
        })?))
    }

    fn try_node_index(self) -> IndexedResult<NodeIndex> {
        Ok(NodeIndex(
            self.try_into()
                .map_err(|_| AnimGraphIndexError::MaxNodesReached)?,
        ))
    }

    fn try_node_type_index(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxNodeTypesReached)
    }

    fn try_branch_index(self) -> IndexedResult<BranchIndex> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxBranchesReached)
    }

    fn try_branch_child_index(self) -> IndexedResult<u8> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxBranchChildrenReached)?
            .try_into()
            .map_err(|_| AnimGraphIndexError::MaxBranchChildrenReached)
    }

    fn try_transition_index(self) -> IndexedResult<IndexType> {
        self.try_into()
            .map_err(|_| AnimGraphIndexError::MaxTransitionsReached)
    }
}

impl IndexConversions for usize {}

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("state machine \"{0}\" has no states")]
    StateMachineHasNoStates(String),
    #[error("duplicate alias: {0}, \"{1}\" and \"{2}\"")]
    DuplicateAliasError(String, String, String),
    #[error("duplicate slot: {0}, \"{1}\" and \"{2}\"")]
    DuplicateSlotError(String, String, String),
    #[error("missing node meta: {0}, \"{1}\"")]
    MissingNodeMetaError(String, String),
    #[error("missing state machine target: {0}, \"{1}\"")]
    MissingStateMachineTargetError(String, String),
    #[error("missing state machine target: {0}, \"{1}\", place machines in depth first order so indices can be translated back to the source")]
    CyclicStateMachineTargetError(String, String),
    #[error("missing root state machine")]
    MissingRootStateMachineError,
    #[error("missing transition target: {0}")]
    MissingTransitionTargetError(String),
    #[error("compilation failure: {0:?}")]
    IndexError(#[from] AnimGraphIndexError),
    #[error("io error: {0:?}")]
    NodeInputError(#[from] AnimGraphNodeInputError),
    #[error("transition condition error in {0}: {1:?}")]
    TransitionConditionError(String, AnimGraphExpressionError),
    #[error("global transition condition error in {0}: {1:?}")]
    GlobalTransitionConditionError(String, AnimGraphExpressionError),
    #[error("internal branch index error")]
    BranchIndexError,

    #[error("graph definition error: {0:?}")]
    GraphDefinitionError(#[from] GraphBuilderError),

    #[error("graph definition error: {0:?}")]
    NodeBuilderError(#[from] NodeCompilationError),

    #[error("missing resource reference({0}): \"{1}\" for \"{2}\"")]
    MissingResourceReference(String, String, String),

    #[error("expected resource {0} got {1}, for \"{2}\" in \"{3}\"")]
    UnexpectedResourceType(String, String, String, String),
}

#[derive(Default, PartialEq, Clone)]
pub struct IndexedPath<'a> {
    pub scope: Vec<(Option<usize>, &'a str)>,
    pub path: Vec<(Option<usize>, &'a str)>,
}

#[derive(PartialEq, Clone)]
pub enum ContextPath<'a> {
    ParentOf(&'a IndexedPath<'a>),
    Path(&'a IndexedPath<'a>),
}

impl<'a> ContextPath<'a> {
    pub fn append(&self, route: &'a str) -> IndexedPath<'a> {
        if route.starts_with("::") {
            return IndexedPath::from_route(route);
        }

        let mut context = match *self {
            ContextPath::ParentOf(child) => {
                let mut parent = child.clone();
                debug_assert!(parent.path.pop().is_some());
                parent
            }
            ContextPath::Path(context) => context.clone(),
        };

        context.append_path(route);
        context
    }

    pub fn inner(&self) -> &IndexedPath<'a> {
        match self {
            ContextPath::ParentOf(inner) => inner,
            ContextPath::Path(inner) => inner,
        }
    }
}

impl<'a> Debug for IndexedPath<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexedPath")
            .field("scope", &self.scope())
            .field("path", &self.path())
            .finish()
    }
}

fn indexed_path(path: &str) -> (Option<usize>, &str) {
    if let Some((part, index)) = path.split_once('#') {
        if let Ok(i) = index.parse::<usize>() {
            return (Some(i), part);
        }
        if let Ok(i) = part.parse::<usize>() {
            return (Some(i), index);
        }
    }

    if let Ok(i) = path.parse::<usize>() {
        (Some(i), "")
    } else {
        (None, path)
    }
}

impl<'a> IndexedPath<'a> {
    pub fn append_path(&mut self, mut value: &'a str) {
        while let Some((path, rest)) = value.split_once('/') {
            value = rest;
            let (index, part) = indexed_path(path);
            self.push(index, part);
        }

        if !value.is_empty() {
            let (index, part) = indexed_path(value);
            self.push(index, part);
        }
    }
    pub fn from_route(mut value: &'a str) -> Self {
        let mut result = IndexedPath::default();

        if value.starts_with("::") {
            value = &value[2..];
            let mut scope = if let Some((scope, rest)) = value.split_once('/') {
                value = rest;
                scope
            } else {
                let scope = value;
                value = "";
                scope
            };

            while let Some((path, rest)) = scope.split_once("::") {
                scope = rest;

                let (index, part) = indexed_path(path);
                result.add_scope(index, part);
            }

            let (index, part) = indexed_path(scope);
            result.add_scope(index, part);
        }

        result.append_path(value);
        result
    }

    fn join_scope(&self) -> String {
        let mut result = String::default();

        for (index, path) in self.scope.iter() {
            result.push_str("::");
            if let Some(index) = index {
                if path.is_empty() {
                    result.push_str(&format!("{index}"));
                } else {
                    result.push_str(&format!("{index}#{path}"));
                }
            } else if !path.is_empty() {
                result.push_str(path);
            } else {
                result.push_str("<null>");
            }
        }

        result
    }

    fn join_path(&self, result: &mut Vec<String>) {
        for (index, path) in self.path.iter() {
            if let Some(index) = index {
                if path.is_empty() {
                    result.push(format!("{index}"));
                } else {
                    result.push(format!("{index}#{path}"));
                }
            } else if !path.is_empty() {
                result.push((*path).to_owned());
            } else {
                result.push("<null>".to_owned());
            }
        }
    }

    pub fn scope(&self) -> String {
        self.join_scope()
    }

    pub fn path(&self) -> String {
        let mut result = Vec::default();
        self.join_path(&mut result);
        result.join("/")
    }

    pub fn join(&self) -> String {
        let mut result = Vec::default();
        if !self.scope.is_empty() {
            result.push(self.join_scope());
        }
        self.join_path(&mut result);
        result.join("/")
    }

    pub fn with_slot(&self, name: &str) -> String {
        if name.is_empty() {
            self.join()
        } else {
            let path = self.path();
            let scope = self.scope();
            if path.is_empty() {
                format!("{scope}.{name}")
            } else {
                format!("{scope}/{path}.{name}")
            }
        }
    }

    pub fn add_scope(&mut self, index: Option<usize>, path: &'a str) {
        self.scope.push((index, path));
    }

    pub fn remove_scope(&mut self, index: Option<usize>, path: &'a str) {
        debug_assert_eq!(self.scope.pop(), Some((index, path)));
    }

    pub fn push(&mut self, index: Option<usize>, path: &'a str) {
        self.path.push((index, path));
    }

    pub fn pop(&mut self, index: Option<usize>, path: &'a str) {
        debug_assert_eq!(self.path.pop(), Some((index, path)));
    }
}

fn build_tree_scope<'a>(
    paths: &mut IndexedPath<'a>,
    qualified_nodes: &mut Vec<NodeCompilationContext<'a>>,
    aliases: &mut HashMap<String, NodeIndex>,
    this_node: &'a Node,
    this_node_index: NodeIndex,
    registry: &'a NodeCompilationRegistry,
) -> Result<Range<NodeIndex>, CompileError> {
    use CompileError::*;
    let start = qualified_nodes.len().try_node_index()?;
    for (index, child) in this_node.children.iter().enumerate() {
        paths.push(Some(index), &child.name);
        let qualified_path = paths.clone();

        let global_index = qualified_nodes.len().try_node_index()?;
        if !child.alias.is_empty() {
            if let Some(index) = aliases.insert(child.alias.clone(), global_index) {
                let dup1 = qualified_nodes
                    .get(index.0 as usize)
                    .map(|x| x.path.join())
                    .unwrap_or_default();
                let dup2 = qualified_nodes
                    .get(global_index.0 as usize)
                    .map(|x| x.path.join())
                    .unwrap_or_default();
                return Err(DuplicateAliasError(child.alias.clone(), dup1, dup2));
            }
        }

        if let Some(meta) = registry.get(&child.name) {
            qualified_nodes.push(NodeCompilationContext {
                path: qualified_path,
                meta,
                node: child,
                parent: Some(this_node_index),
                children: None,
                default_output: None,
                outputs: Default::default(),
                default_input: None,
                inputs: Default::default(),
            });
        } else {
            return Err(MissingNodeMetaError(
                child.name.clone(),
                qualified_path.join(),
            ));
        }

        paths.pop(Some(index), &child.name);
    }
    let end = qualified_nodes.len().try_node_index()?;

    for (index, (child, global)) in this_node
        .children
        .iter()
        .zip(start.0 as usize..end.0 as usize)
        .enumerate()
    {
        paths.push(Some(index), &child.name);
        let range = build_tree_scope(
            paths,
            qualified_nodes,
            aliases,
            child,
            NodeIndex(global as _),
            registry,
        )?;
        paths.pop(Some(index), &child.name);

        let context = qualified_nodes.get_mut(global).expect("Valid");
        debug_assert!(context.children.is_none());
        if !range.is_empty() {
            context.children = Some(range);
        }
    }

    Ok(start..end)
}

fn build_tree_root<'a>(
    paths: &mut IndexedPath<'a>,
    qualified_nodes: &mut Vec<NodeCompilationContext<'a>>,
    aliases: &mut HashMap<String, NodeIndex>,
    node: &'a Node,
    registry: &'a NodeCompilationRegistry,
) -> Result<NodeIndex, CompileError> {
    use CompileError::*;
    paths.push(Some(0), TREE_ROOT_SCOPE);

    let qualified_path = paths.clone();

    let global_index = qualified_nodes.len().try_node_index()?;
    if let Some(meta) = registry.get(&node.name) {
        qualified_nodes.push(NodeCompilationContext {
            path: qualified_path,
            meta,
            node,
            parent: None,
            children: None,
            default_output: None,
            outputs: Default::default(),
            default_input: None,
            inputs: Default::default(),
        });
    } else {
        return Err(MissingNodeMetaError(
            node.name.clone(),
            qualified_path.join(),
        ));
    }

    if !node.alias.is_empty() {
        if let Some(index) = aliases.insert(node.alias.clone(), global_index) {
            let dup1 = qualified_nodes
                .get(index.0 as usize)
                .map(|x| x.path.join())
                .unwrap_or_default();
            let dup2 = qualified_nodes
                .get(global_index.0 as usize)
                .map(|x| x.path.join())
                .unwrap_or_default();
            return Err(DuplicateAliasError(node.alias.clone(), dup1, dup2));
        }
    }

    let range = build_tree_scope(
        paths,
        qualified_nodes,
        aliases,
        node,
        global_index,
        registry,
    )?;

    let context = qualified_nodes
        .get_mut(global_index.0 as usize)
        .expect("Valid");
    debug_assert!(context.children.is_none());

    if !range.is_empty() {
        context.children = Some(range);
    }

    paths.pop(Some(0), TREE_ROOT_SCOPE);
    Ok(global_index)
}

fn build_branch_scope<'a>(
    branches: &mut Vec<BranchCompilationContext<'a>>,
    paths: &mut IndexedPath<'a>,
    qualified_nodes: &mut Vec<NodeCompilationContext<'a>>,
    aliases: &mut HashMap<String, NodeIndex>,
    branch: &'a Branch,
    registry: &'a NodeCompilationRegistry,
    graph: &'a AnimGraph,
    processed_machines: &mut HashMap<&'a str, MachineIndex>,
) -> Result<HierarchicalBranch, CompileError> {
    use CompileError::*;
    let node_index = if let Some(node) = branch.node.as_ref() {
        Some(build_tree_root(
            paths,
            qualified_nodes,
            aliases,
            node,
            registry,
        )?)
    } else {
        None
    };

    let branch_target: HierarchicalBranchTarget;

    if let Some(target) = branch.target.as_ref() {
        match target {
            BranchTarget::Postprocess(branch) => {
                paths.add_scope(Some(0), "preprocess");
                let qualified_path = paths.clone();
                let global_index: BranchIndex = branches.len().try_branch_index()?;
                branches.push(BranchCompilationContext {
                    path: qualified_path,
                    branch,
                    target: HierarchicalBranch::default(),
                });
                let child_branch = build_branch_scope(
                    branches,
                    paths,
                    qualified_nodes,
                    aliases,
                    branch,
                    registry,
                    graph,
                    processed_machines,
                )?;
                paths.remove_scope(Some(0), "preprocess");
                branches[global_index as usize].target = child_branch;
                branch_target = HierarchicalBranchTarget::PostProcess(global_index);
            }
            BranchTarget::Layers(layers) => {
                let start: BranchIndex = branches.len().try_branch_index()?;
                for (index, layer) in layers.iter().enumerate() {
                    paths.add_scope(Some(index), "");
                    let qualified_path = paths.clone();

                    branches.push(BranchCompilationContext {
                        path: qualified_path,
                        branch: layer,
                        target: HierarchicalBranch::default(),
                    });
                    paths.remove_scope(Some(index), "");
                }
                let end: BranchIndex = branches.len().try_branch_index()?;
                let count: u8 = ((end - start) as usize).try_branch_child_index()?;
                branch_target = HierarchicalBranchTarget::Layers { start, count };

                for ((index, layer), global_index) in
                    layers.iter().enumerate().zip(start as usize..end as usize)
                {
                    paths.add_scope(Some(index), "");
                    let child_branch = build_branch_scope(
                        branches,
                        paths,
                        qualified_nodes,
                        aliases,
                        layer,
                        registry,
                        graph,
                        processed_machines,
                    )?;
                    paths.remove_scope(Some(index), "");

                    let context = branches.get_mut(global_index).expect("Valid");
                    context.target = child_branch;
                }
            }
            BranchTarget::StateMachine(target) => {
                if processed_machines.contains_key(&target.name.as_str()) {
                    return Err(CyclicStateMachineTargetError(
                        target.name.clone(),
                        paths.join(),
                    ));
                }

                if let Some(machine) = graph.index_of_state_machine(&target.name) {
                    branch_target = HierarchicalBranchTarget::Machine(machine.try_machine_index()?);
                } else {
                    return Err(MissingStateMachineTargetError(
                        target.name.clone(),
                        paths.join(),
                    ));
                }
            }
        }
    } else {
        branch_target = HierarchicalBranchTarget::Endpoint;
    }

    Ok(HierarchicalBranch {
        node: node_index,
        target: branch_target,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeCompilationOutput {
    pub output_type: IOType,
    pub index: IndexType,
}

impl NodeCompilationOutput {
    pub fn as_timer(&self) -> Option<Timer> {
        if self.output_type == IOType::Timer {
            Some(Timer(self.index))
        } else {
            None
        }
    }

    pub fn as_event(&self) -> Option<Event> {
        if self.output_type == IOType::Event {
            Some(Event(self.index))
        } else {
            None
        }
    }

    pub fn as_number<T: FromFloatUnchecked>(&self) -> Option<NumberMut<T>> {
        if self.output_type == IOType::Number {
            Some(NumberMut::new(VariableIndex(self.index)))
        } else {
            None
        }
    }

    pub fn as_vector(&self) -> Option<VectorMut> {
        if self.output_type == IOType::Vector {
            Some(VectorMut(VariableIndex(self.index)))
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<BoolMut> {
        if self.output_type == IOType::Bool {
            Some(BoolMut::new(VariableIndex(self.index)))
        } else {
            None
        }
    }

    pub fn as_resource<T: ResourceSettings>(&self) -> Option<Resource<T>> {
        if self.output_type == IOType::Resource(T::resource_type()) {
            Some(Resource::new(VariableIndex(self.index)))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum NodeCompilationInput {
    Boolean(GraphBoolean),
    Number(GraphNumber),
    Vector(VectorRef),
    Event(Event),
    Timer(Timer),
    Resource {
        type_index: IndexType,
        variable: VariableIndex,
    },
}

impl NodeCompilationInput {
    pub fn as_bool(&self) -> Option<GraphBoolean> {
        use NodeCompilationInput::*;
        match self {
            &Boolean(x) => Some(x),
            _ => None,
        }
    }

    pub fn as_number<T: FromFloatUnchecked>(&self) -> Option<NumberRef<T>> {
        use NodeCompilationInput::*;
        match self {
            &Number(x) => Some(NumberRef::new(x)),
            _ => None,
        }
    }

    pub fn as_vector(&self) -> Option<VectorRef> {
        use NodeCompilationInput::*;
        match self {
            &Vector(x) => Some(x),
            _ => None,
        }
    }

    pub fn as_event(&self) -> Option<Event> {
        use NodeCompilationInput::*;
        match self {
            &Event(x) => Some(x),
            _ => None,
        }
    }

    pub fn as_timer(&self) -> Option<Timer> {
        use NodeCompilationInput::*;
        match self {
            &Timer(x) => Some(x),
            _ => None,
        }
    }

    pub fn as_resource<T: ResourceSettings>(&self, resource_type: usize) -> Option<Resource<T>> {
        match self {
            &NodeCompilationInput::Resource {
                type_index,
                variable,
            } if type_index as usize == resource_type => Some(Resource::new(variable)),
            _ => None,
        }
    }
}

pub struct NodeCompilationContext<'a> {
    pub path: IndexedPath<'a>,
    pub meta: &'a dyn NodeCompilationMeta,
    pub node: &'a Node,
    pub parent: Option<NodeIndex>,
    pub children: Option<Range<NodeIndex>>,
    pub default_output: Option<NodeCompilationOutput>,
    pub default_input: Option<NodeCompilationInput>,
    pub outputs: Vec<NodeCompilationOutput>,
    pub inputs: Vec<NodeCompilationInput>,
}

impl<'a> Debug for NodeCompilationContext<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimGraphNodeContext")
            .field("path", &self.path)
            .field("meta", &self.meta.name())
            .field("node", &self.node)
            .field("parent", &self.parent)
            .field("children", &self.children)
            .field("default_output", &self.default_output)
            .field("default_input", &self.default_input)
            .field("outputs", &self.outputs)
            .field("inputs", &self.inputs)
            .finish()
    }
}

pub struct BranchCompilationContext<'a> {
    pub path: IndexedPath<'a>,
    pub branch: &'a Branch,
    pub target: HierarchicalBranch,
}

impl<'a> BranchCompilationContext<'a> {
    pub fn get_branch_or_node_path(
        &'a self,
        graph: &'a GraphCompilationContext<'a>,
    ) -> &'a IndexedPath<'a> {
        if let Some(node) = self.target.node.and_then(|x| graph.nodes.get(x.0 as usize)) {
            &node.path
        } else {
            &self.path
        }
    }
}

pub struct TransitionCompilationContext<'a> {
    pub path: IndexedPath<'a>,
    pub transition: &'a Transition,
    pub source_index: StateIndex,
    pub target_index: StateIndex,
    pub node: Option<NodeIndex>,
}

pub struct StateCompilationContext<'a> {
    pub path: IndexedPath<'a>,
    pub state: &'a State,
    pub branch_index: BranchIndex,
    pub transitions: Range<IndexType>,
}

pub struct StateMachineCompilationContext<'a> {
    pub state_machine: &'a StateMachine,
    pub states: HashMap<String, StateIndex>,
    pub state_range: Range<StateIndex>,
    pub total_transition_range: Range<IndexType>,
}

#[derive(Clone, Default)]
pub struct NodeCompilationRegistry {
    pub nodes: HashMap<String, Arc<dyn NodeCompilationMeta>>,
}

impl NodeCompilationRegistry {
    pub fn get(&self, name: &str) -> Option<&dyn NodeCompilationMeta> {
        self.nodes.get(name).map(|x| x.as_ref())
    }

    pub fn register<T: NodeCompiler + Send + Sync + 'static>(&mut self) {
        self.nodes.insert(
            T::Settings::name().to_owned(),
            Arc::new(NodeCompilerMeta::<T>(PhantomData)),
        );
    }
}

pub struct GraphCompilationContext<'a> {
    pub graph: &'a AnimGraph,
    pub registry: &'a NodeCompilationRegistry,
    pub machines: Vec<StateMachineCompilationContext<'a>>,
    pub states: Vec<StateCompilationContext<'a>>,
    pub transitions: Vec<TransitionCompilationContext<'a>>,
    pub branches: Vec<BranchCompilationContext<'a>>,
    pub nodes: Vec<NodeCompilationContext<'a>>,
    pub aliases: HashMap<String, NodeIndex>,
}

#[derive(Error, Debug)]
pub enum AnimGraphPathError {
    #[error("path error unknown machine: {0}")]
    UnknownMachine(String),
    #[error("path error unknown state: {0}")]
    UnknownState(String),
    #[error("no such node slot path: {0}")]
    InvalidNodeSlot(String),
    #[error("no such node path: {0}")]
    InvalidNode(String),
    #[error("no such branch path: {0}")]
    InvalidBranch(String),
    #[error("no such transition path: {0}")]
    InvalidTransition(String),
    #[error("no such route: {0}")]
    InvalidRoute(String),
}

impl<'a> GraphCompilationContext<'a> {
    pub fn resolve_root_path(
        &self,
        route: &IndexedPath,
        output_slot: &str,
    ) -> Result<NodeCompilationOutput, AnimGraphPathError> {
        let mut path = route.path.as_slice();
        // requires atleast ::<Machine>::<State>::branch/node
        // or ::<Machine>::<State>::transition::<State>/node
        if path.is_empty() || route.scope.len() < 3 {
            return Err(AnimGraphPathError::InvalidRoute(
                route.with_slot(output_slot),
            ));
        }

        let (machine_index, machine_name) = route.scope[0];
        let machine_index = if let Some(index) = self
            .graph
            .state_machines
            .iter()
            .position(|x| x.name == machine_name)
        {
            index
        } else if let Some(index) = machine_index {
            index
        } else {
            return Err(AnimGraphPathError::UnknownMachine(
                route.with_slot(output_slot),
            ));
        };

        let state_machine = self
            .machines
            .get(machine_index)
            .ok_or_else(|| AnimGraphPathError::UnknownMachine(route.with_slot(output_slot)))?;

        let (state_index, state_name) = route.scope[1];
        let state_index = if let Some(index) = state_machine.states.get(state_name) {
            index.0 as usize
        } else if let Some(index) = state_index {
            let count = state_machine.state_range.end.0 as usize
                - state_machine.state_range.start.0 as usize;
            if index >= count {
                return Err(AnimGraphPathError::UnknownState(
                    route.with_slot(output_slot),
                ));
            }
            state_machine.state_range.start.0 as usize + index
        } else {
            return Err(AnimGraphPathError::UnknownState(
                route.with_slot(output_slot),
            ));
        };

        let state = self
            .states
            .get(state_index)
            .ok_or_else(|| AnimGraphPathError::UnknownState(route.with_slot(output_slot)))?;

        let (scope_index, state_scope) = route.scope[2];

        let relative_to_node;
        // 0 or branch
        if scope_index == Some(0) || state_scope == BRANCH_SCOPE {
            if !state_scope.is_empty() && state_scope != BRANCH_SCOPE {
                return Err(AnimGraphPathError::InvalidRoute(
                    route.with_slot(output_slot),
                ));
            }

            let mut branch_index = state.branch_index;
            for (index, _name) in route.scope.iter().skip(3) {
                let branch = self.branches.get(branch_index as usize).ok_or_else(|| {
                    AnimGraphPathError::InvalidBranch(route.with_slot(output_slot))
                })?;
                let child = if let Some(child) = index {
                    *child
                } else {
                    return Err(AnimGraphPathError::InvalidBranch(
                        route.with_slot(output_slot),
                    ));
                };

                match branch.target.target {
                    HierarchicalBranchTarget::Layers { start, count }
                        if child <= count as usize =>
                    {
                        branch_index = (start as usize + child) as _;
                    }
                    _ => {
                        return Err(AnimGraphPathError::InvalidBranch(
                            route.with_slot(output_slot),
                        ))
                    }
                }
            }

            let branch = self
                .branches
                .get(branch_index as usize)
                .ok_or_else(|| AnimGraphPathError::InvalidBranch(route.with_slot(output_slot)))?;

            relative_to_node = branch.target.node;

            if relative_to_node.is_none() {
                return Err(AnimGraphPathError::InvalidBranch(
                    route.with_slot(output_slot),
                ));
            }
        } else if scope_index == Some(1) || state_scope == TRANSITION_SCOPE {
            if !state_scope.is_empty() && state_scope != TRANSITION_SCOPE {
                return Err(AnimGraphPathError::InvalidRoute(
                    route.with_slot(output_slot),
                ));
            }

            // ::<Machine>::<State>::transition::<State>/node
            if route.scope.len() != 4 || state.transitions.is_empty() {
                return Err(AnimGraphPathError::InvalidTransition(
                    route.with_slot(output_slot),
                ));
            }

            let (transition_index, transition_scope) = route.scope[3];

            if let Some(transition_index) = transition_index {
                let transition = self
                    .transitions
                    .get(state.transitions.start as usize + transition_index)
                    .ok_or_else(|| {
                        AnimGraphPathError::InvalidTransition(route.with_slot(output_slot))
                    })?;

                if !transition_scope.is_empty()
                    && transition.transition.target.name != transition_scope
                {
                    return Err(AnimGraphPathError::InvalidTransition(
                        route.with_slot(output_slot),
                    ));
                }

                relative_to_node = transition.node;
            } else if !transition_scope.is_empty() {
                let transitions = self
                    .transitions
                    .get(state.transitions.start as usize..state.transitions.end as usize)
                    .ok_or_else(|| {
                        AnimGraphPathError::InvalidTransition(route.with_slot(output_slot))
                    })?;
                relative_to_node = 'found: {
                    for transition in transitions.iter() {
                        if transition.transition.target.name == transition_scope {
                            break 'found transition.node;
                        }
                    }
                    return Err(AnimGraphPathError::InvalidTransition(
                        route.with_slot(output_slot),
                    ));
                };
            } else {
                return Err(AnimGraphPathError::InvalidTransition(
                    route.with_slot(output_slot),
                ));
            }

            // ::<Machine>::<State>::transition::<State>/node
            if relative_to_node.is_none() {
                return Err(AnimGraphPathError::InvalidTransition(
                    route.with_slot(output_slot),
                ));
            }
        } else {
            return Err(AnimGraphPathError::InvalidRoute(
                route.with_slot(output_slot),
            ));
        }

        // First path part must be into the node for the branch or transition
        let node_index = if let Some(node_index) = relative_to_node {
            match path.first() {
                Some(&(index, name))
                    if (name.is_empty() || (TREE_ROOT_SCOPE == name))
                        && (index.is_none() || index == Some(0)) => {}
                _ => {
                    return Err(AnimGraphPathError::InvalidRoute(
                        route.with_slot(output_slot),
                    ))
                }
            }
            path = &path[1..];
            node_index
        } else {
            return Err(AnimGraphPathError::InvalidRoute(
                route.with_slot(output_slot),
            ));
        };

        self.resolve_node_relative(route, node_index, path, output_slot)
    }

    fn resolve_node_relative(
        &self,
        route: &IndexedPath,
        mut node_index: NodeIndex,
        mut path: &[(Option<usize>, &str)],
        output_slot: &str,
    ) -> Result<NodeCompilationOutput, AnimGraphPathError> {
        while let Some(&(index, name)) = path.first() {
            path = &path[1..];
            let node = self
                .nodes
                .get(node_index.0 as usize)
                .ok_or_else(|| AnimGraphPathError::InvalidRoute(route.with_slot(output_slot)))?;

            let children = if let Some(range) = node.children.clone() {
                range
            } else {
                return Err(AnimGraphPathError::InvalidNode(
                    route.with_slot(output_slot),
                ));
            };

            node_index = 'found: {
                for ((child_index, child), child_global_index) in node
                    .node
                    .children
                    .iter()
                    .enumerate()
                    .zip(children.start.0..children.end.0)
                {
                    let child_name = &child.name;
                    if index == Some(child_index) || name == child_name {
                        if (index.is_some() && index != Some(child_index))
                            || (!name.is_empty() && name != child_name)
                        {
                            return Err(AnimGraphPathError::InvalidNode(
                                route.with_slot(output_slot),
                            ));
                        }

                        break 'found NodeIndex(child_global_index);
                    }
                }

                return Err(AnimGraphPathError::InvalidRoute(
                    route.with_slot(output_slot),
                ));
            };
        }

        self.nodes
            .get(node_index.0 as usize)
            .and_then(|x| {
                if output_slot == DEFAULT_OUPTUT_NAME {
                    x.default_output.clone()
                } else {
                    for (slot, &(slot_name, _)) in x.meta.output().iter().enumerate() {
                        if output_slot == slot_name {
                            return Some(x.outputs.get(slot).expect("Valid slot").clone());
                        }
                    }
                    None
                }
            })
            .ok_or_else(|| AnimGraphPathError::InvalidRoute(route.with_slot(output_slot)))
    }

    pub fn resolve_route(
        &self,
        context: &ContextPath,
        mut route: &str,
    ) -> Result<NodeCompilationOutput, AnimGraphPathError> {
        let mut output_slot = DEFAULT_OUPTUT_NAME;
        if let Some(index) = route.rfind('.') {
            let (owner, mut slot) = route.split_at(index);
            assert!(slot.starts_with('.'));
            slot = &slot[1..];
            if !slot.contains("::") && !slot.contains('/') {
                output_slot = slot;
                route = owner;
            }
        }

        if !route.starts_with("::") && !route.starts_with('/') {
            if let Some((alias, _path)) = route.split_once('/') {
                if let Some(&node_index) = self.aliases.get(alias) {
                    let route = IndexedPath::from_route(route);

                    if route.path.len() < 2 {
                        return Err(AnimGraphPathError::InvalidRoute(route.join()));
                    }

                    let path = &route.path[1..];

                    return self.resolve_node_relative(&route, node_index, path, output_slot);
                }
            } else if let Some(&node_index) = self.aliases.get(route) {
                return self
                    .nodes
                    .get(node_index.0 as usize)
                    .and_then(|x| {
                        if output_slot == DEFAULT_OUPTUT_NAME {
                            x.default_output.clone()
                        } else {
                            for (slot, &(slot_name, _)) in x.meta.output().iter().enumerate() {
                                if output_slot == slot_name {
                                    return Some(x.outputs.get(slot).expect("Valid slot").clone());
                                }
                            }
                            None
                        }
                    })
                    .ok_or_else(|| {
                        AnimGraphPathError::InvalidRoute(if output_slot.is_empty() {
                            route.to_owned()
                        } else {
                            format!("{}.{}", route, output_slot)
                        })
                    });
            }
        }
        self.resolve_root_path(&context.append(route), output_slot)
    }
}

#[derive(Error, Debug)]
pub enum AnimGraphNodeInputError {
    #[error("unexpected node input {0} expected: {1:?} got {2:?}")]
    UnexpectedType(String, IOType, IOType),
    #[error("unexpected node input {0} expected: {1:?} got {2:?}")]
    UnexpectedValue(String, IOType, Value),
    #[error("unexpected node input {0} expected: {1:?} got {2:?}")]
    ConstantOutOfRange(String, IOType, Value),
    #[error("route error: {2:?} for node input {0} ({1:?})")]
    RouteError(String, IOType, AnimGraphPathError),
    #[error("expression error: {2:?} for node input {0} ({1:?})")]
    ExpressionError(String, IOType, AnimGraphExpressionError),
}

#[derive(Error, Debug)]
pub enum AnimGraphExpressionError {
    #[error("unexpected empty expression")]
    EmptyExpression,
    #[error("expected value {0:?} to be boolean")]
    InvalidBooleanValue(Value),
    #[error("expected parameter {0} to be boolean")]
    ExpectedBooleanParameter(String),
    #[error("invalid boolean route: {0:?}")]
    InvalidBooleanRoute(AnimGraphPathError),
    #[error("invalid boolean expression: {0:?}")]
    InvalidBooleanExpression(Expression),
    #[error("invalid boolean route: {0:?}")]
    ExpectedBooleanRoute(IORouteSettings),
    #[error("cannot resolve expression {1:?} as {0:?}")]
    CannotResolveExpression(IOType, Expression),
    #[error("invalid global boolean: {0}")]
    InvalidGlobalBoolean(String),
    #[error("invalid state machine: {0}")]
    InvalidStateMachine(String),
    #[error("invalid state path: {0}")]
    InvalidStatePath(String),

    #[error(transparent)]
    IndexError(#[from] AnimGraphIndexError),

    #[error("condition expression not allow (internal error)")]
    ConditionExpressionNotAllowed,

    #[error("Expected number expression: {0:?}")]
    ExpectedNumber(Expression),
    #[error("Expected vector expression: {0:?}")]
    ExpectedVector(Expression),
    #[error("Expected number expression: {0:?}")]
    ExpectedNumberValue(Value),
    #[error("invalid route (number): {0:?}")]
    InvalidNumberRoute(AnimGraphPathError),
    #[error("invalid route (vector): {0:?}")]
    InvalidVectorRoute(AnimGraphPathError),
    #[error("invalid number route: {0:?}")]
    ExpectedNumberRoute(IORouteSettings),
    #[error("invalid vector route: {0:?}")]
    ExpectedVectorRoute(IORouteSettings),
    #[error("invalid global number: {0}")]
    InvalidGlobalNumber(String),

    #[error("Equality comparison failed lhs: {0:?} == {1:?}, error: {2:?}")]
    EqualityComparisonError(Expression, Expression, Box<AnimGraphExpressionError>),

    #[error("Number expression {0:?} error: {1:?}")]
    NumberExpressionError(Expression, Box<AnimGraphExpressionError>),

    #[error("Contains exclusive failed: {0:?}, error: {1:?}")]
    ContainsExclusiveError(
        (Expression, Expression, Expression),
        Box<AnimGraphExpressionError>,
    ),
    #[error("Contains exclusive failed: {0:?}, error: {1:?}")]
    ContainsInclusiveError(
        (Expression, Expression, Expression),
        Box<AnimGraphExpressionError>,
    ),
}

impl GraphDefinitionCompilation {
    fn create_output(
        &mut self,
        name: &str,
        io_type: IOType,
    ) -> IndexedResult<NodeCompilationOutput> {
        let index;
        match io_type {
            IOType::Bool => {
                index = self.builder.booleans;
                self.builder.booleans = (self.builder.booleans as usize + 1).try_boolean()?;
            }
            IOType::Number => {
                index = self.builder.numbers;
                self.builder.numbers = (self.builder.numbers as usize + 1).try_number()?;
            }
            IOType::Vector => {
                index = self.builder.numbers;
                self.builder.numbers = (self.builder.numbers as usize + 3).try_vector()?;
            }
            IOType::Event => {
                index = self.builder.events.len().try_event()?;
                self.builder.events.push(Id::from_str(name));
                self.events.push(name.to_owned());
            }
            IOType::Timer => {
                index = self.builder.timers;
                self.builder.timers = (self.builder.timers as usize + 1).try_timer()?;
            }
            IOType::Resource(res_type) => {
                let resource_type = self.get_or_create_resource_type(res_type)?;
                index = self.builder.resources.len().try_resource_variable()?;
                self.builder.resources.push(GraphResourceEntry {
                    resource_type,
                    initial: None,
                });
            }
        }
        let result = NodeCompilationOutput {
            index,
            output_type: io_type,
        };

        Ok(result)
    }

    fn get_or_create_resource_type(&mut self, resource_type: &str) -> IndexedResult<IndexType> {
        if let Some(index) = self
            .builder
            .resource_types
            .iter()
            .position(|x| x == resource_type)
        {
            return index.try_resource_type();
        }

        let index = self.builder.resource_types.len().try_resource_type()?;
        self.builder.resource_types.push(resource_type.to_owned());
        Ok(index)
    }

    fn resolve_vector_variable(
        &mut self,
        expression: &Expression,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
    ) -> Result<VectorMut, AnimGraphExpressionError> {
        match expression {
            Expression::Route(route) => match graph.resolve_route(context_path, &route.route) {
                Ok(NodeCompilationOutput {
                    output_type: IOType::Vector,
                    index,
                }) => Ok(VectorMut(VariableIndex(index))),
                Ok(_) => Err(AnimGraphExpressionError::ExpectedVectorRoute(route.clone())),
                Err(inner) => Err(AnimGraphExpressionError::InvalidVectorRoute(inner)),
            },
            Expression::Parameter(parameter) => {
                let NodeCompilationOutput { index, .. } =
                    self.get_or_create_parameter(&parameter.name, &IOType::Vector)?;
                Ok(VectorMut(VariableIndex(index)))
            }
            Expression::None
            | Expression::Binary(..)
            | Expression::ContainsExclusive(..)
            | Expression::ContainsInclusive(..)
            | Expression::Value(_)
            | Expression::Query(_)
            | Expression::CompilerGlobal(_)
            | Expression::VectorProjection(_, _)
            | Expression::Not(_)
            | Expression::CompareBoolean(_)
            | Expression::CompareNumber(_)
            | Expression::Less(_)
            | Expression::Greater(_)
            | Expression::And(_)
            | Expression::Or(_)
            | Expression::Xor(_)
            | Expression::Debug { .. } => {
                Err(AnimGraphExpressionError::ExpectedVector(expression.clone()))
            }
        }
    }

    fn resolve_number_expression(
        &mut self,
        expression: &Expression,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
    ) -> Result<GraphNumber, AnimGraphExpressionError> {
        match expression {
            Expression::Value(value) => match value {
                Value::Number(number) => {
                    match self.get_or_create_number_constant(number.clone())? {
                        Some(index) => Ok(index),
                        None => Err(AnimGraphExpressionError::ExpectedNumberValue(value.clone())),
                    }
                }
                _ => Err(AnimGraphExpressionError::ExpectedNumberValue(value.clone())),
            },
            Expression::Route(route) => match graph.resolve_route(context_path, &route.route) {
                Ok(NodeCompilationOutput {
                    output_type: IOType::Number,
                    index,
                }) => Ok(GraphNumber::Variable(VariableIndex(index))),
                Ok(_) => Err(AnimGraphExpressionError::ExpectedNumberRoute(route.clone())),
                Err(inner) => Err(AnimGraphExpressionError::InvalidNumberRoute(inner)),
            },
            Expression::Parameter(parameter) => {
                let NodeCompilationOutput { index, .. } =
                    self.get_or_create_parameter(&parameter.name, &IOType::Number)?;
                Ok(GraphNumber::Variable(VariableIndex(index)))
            }
            Expression::CompilerGlobal(name) => {
                if let Some(index) = self.global_number.get(name) {
                    Ok(*index)
                } else {
                    Err(AnimGraphExpressionError::InvalidGlobalNumber(name.clone()))
                }
            }
            Expression::VectorProjection(projection, expr) => {
                let number = GraphNumber::Projection(
                    *projection,
                    self.resolve_vector_variable(expr, graph, context_path)?.0,
                );

                Ok(number)
            }

            Expression::Binary(op, e) => {
                let index = self.builder.desc.conditions.len().try_expression()?;
                self.builder
                    .desc
                    .expressions
                    .push(GraphExpression::Expression(GraphNumberExpression::Binary(
                        op.clone(),
                        GraphNumber::Zero,
                        GraphNumber::Zero,
                    )));

                match (
                    self.resolve_number_expression(&e.0, graph, context_path),
                    self.resolve_number_expression(&e.1, graph, context_path),
                ) {
                    (Ok(lhs), Ok(rhs)) => {
                        self.builder.desc.expressions[index as usize] = GraphExpression::Expression(
                            GraphNumberExpression::Binary(op.clone(), lhs, rhs),
                        );
                        Ok(GraphNumber::Expression(index))
                    }
                    (Err(err), ..) | (_, Err(err)) => {
                        Err(AnimGraphExpressionError::NumberExpressionError(
                            expression.clone(),
                            Box::new(err),
                        ))
                    }
                }
            }
            Expression::None
            | Expression::ContainsExclusive(..)
            | Expression::ContainsInclusive(..)
            | Expression::Query(_)
            | Expression::Not(_)
            | Expression::CompareBoolean(_)
            | Expression::CompareNumber(_)
            | Expression::Less(_)
            | Expression::Greater(_)
            | Expression::And(_)
            | Expression::Or(_)
            | Expression::Xor(_)
            | Expression::Debug { .. } => {
                Err(AnimGraphExpressionError::ExpectedNumber(expression.clone()))
            }
        }
    }

    fn resolve_condition_expression_list(
        &mut self,
        list: &Vec<Expression>,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
    ) -> Result<Range<IndexType>, AnimGraphExpressionError> {
        let start = self.builder.desc.conditions.len();
        self.builder.desc.conditions.resize(
            self.builder.desc.conditions.len() + list.len(),
            GraphCondition::never(),
        );
        let end = self.builder.desc.conditions.len();

        for (index, expr) in (start..end).zip(list.iter()) {
            self.builder.desc.conditions[index] =
                self.resolve_condition_expression(expr, graph, context_path)?;
        }

        let a: IndexType = start.try_condition()?;
        let b: IndexType = end.try_condition()?;
        Ok(a..b)
    }

    fn resolve_condition_expression(
        &mut self,
        expression: &Expression,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
    ) -> Result<GraphCondition, AnimGraphExpressionError> {
        match expression {
            Expression::Less(expr) => {
                let lhs = self.resolve_number_expression(&expr.0, graph, context_path)?;
                let rhs = self.resolve_number_expression(&expr.1, graph, context_path)?;

                Ok(GraphCondition::strictly_less(lhs, rhs))
            }
            Expression::Greater(expr) => {
                let lhs = self.resolve_number_expression(&expr.0, graph, context_path)?;
                let rhs = self.resolve_number_expression(&expr.1, graph, context_path)?;
                Ok(GraphCondition::strictly_greater(lhs, rhs))
            }
            Expression::Not(expr) => {
                let condition = self.resolve_condition_expression(expr, graph, context_path)?;
                Ok(condition.not())
            }
            Expression::CompareBoolean(expr) => {
                match (
                    self.resolve_boolean_expression(&expr.0, graph, context_path, true),
                    self.resolve_boolean_expression(&expr.1, graph, context_path, true),
                ) {
                    (Ok(lhs), Ok(rhs)) => Ok(GraphCondition::equal(lhs, rhs)),
                    (Err(err), ..) | (Ok(_), Err(err)) => {
                        Err(AnimGraphExpressionError::EqualityComparisonError(
                            expr.0.clone(),
                            expr.1.clone(),
                            Box::new(err),
                        ))
                    }
                }
            }
            Expression::CompareNumber(expr) => {
                match (
                    self.resolve_number_expression(&expr.0, graph, context_path),
                    self.resolve_number_expression(&expr.1, graph, context_path),
                ) {
                    (Ok(lhs), Ok(rhs)) => Ok(GraphCondition::like(lhs, rhs)),
                    (Err(err), ..) | (_, Err(err)) => {
                        Err(AnimGraphExpressionError::EqualityComparisonError(
                            expr.0.clone(),
                            expr.1.clone(),
                            Box::new(err),
                        ))
                    }
                }
            }
            Expression::And(list) => {
                let children = self.resolve_condition_expression_list(list, graph, context_path)?;
                Ok(GraphCondition::all_of(children))
            }
            Expression::Or(list) => {
                let children = self.resolve_condition_expression_list(list, graph, context_path)?;
                Ok(GraphCondition::any_true(children))
            }
            Expression::Xor(list) => {
                let children = self.resolve_condition_expression_list(list, graph, context_path)?;
                Ok(GraphCondition::exlusive_or(children))
            }
            Expression::Debug { trigger, condition } => {
                let start: IndexType = self.builder.desc.conditions.len().try_condition()?;

                self.builder.desc.conditions.push(GraphCondition::never());

                let end: IndexType = self.builder.desc.conditions.len().try_condition()?;

                let condition = self
                    .resolve_condition_expression(condition, graph, context_path)?
                    .debug_break();

                self.builder.desc.conditions[start as usize] = condition;
                self.debug_triggers.insert(start, trigger.clone());

                Ok(GraphCondition::all_of(start..end))
            }
            Expression::ContainsExclusive(expr) => {
                match (
                    self.resolve_number_expression(&expr.0, graph, context_path),
                    self.resolve_number_expression(&expr.1, graph, context_path),
                    self.resolve_number_expression(&expr.2, graph, context_path),
                ) {
                    (Ok(lhs), Ok(rhs), Ok(value)) => {
                        Ok(GraphCondition::contains_exclusive((lhs, rhs), value))
                    }
                    (_, Err(err), _) | (Err(err), ..) | (.., Err(err)) => {
                        return Err(AnimGraphExpressionError::ContainsExclusiveError(
                            expr.as_ref().clone(),
                            Box::new(err),
                        ))
                    }
                }
            }
            Expression::ContainsInclusive(expr) => {
                match (
                    self.resolve_number_expression(&expr.0, graph, context_path),
                    self.resolve_number_expression(&expr.1, graph, context_path),
                    self.resolve_number_expression(&expr.2, graph, context_path),
                ) {
                    (Ok(lhs), Ok(rhs), Ok(value)) => {
                        Ok(GraphCondition::contains_inclusive((lhs, rhs), value))
                    }
                    (_, Err(err), _) | (Err(err), ..) | (.., Err(err)) => {
                        return Err(AnimGraphExpressionError::ContainsInclusiveError(
                            expr.as_ref().clone(),
                            Box::new(err),
                        ))
                    }
                }
            }

            Expression::None
            | Expression::Value(_)
            | Expression::Route(_)
            | Expression::Parameter(_)
            | Expression::Query(_)
            | Expression::Binary(..)
            | Expression::VectorProjection(..)
            | Expression::CompilerGlobal(_) => {
                let result =
                    self.resolve_boolean_expression(expression, graph, context_path, false)?;
                Ok(GraphCondition::unary_true(result))
            }
        }
    }

    fn resolve_boolean_expression(
        &mut self,
        expression: &Expression,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
        allow_condition: bool, // stops accidental infinite recursion
    ) -> Result<GraphBoolean, AnimGraphExpressionError> {
        match expression {
            Expression::None => Ok(GraphBoolean::Never),
            Expression::Value(value) => match value {
                Value::Null => Ok(GraphBoolean::Never),
                Value::Bool(x) => {
                    if *x {
                        Ok(GraphBoolean::Always)
                    } else {
                        Ok(GraphBoolean::Never)
                    }
                }
                _ => Err(AnimGraphExpressionError::InvalidBooleanValue(value.clone())),
            },
            Expression::Route(route) => match graph.resolve_route(context_path, &route.route) {
                Ok(NodeCompilationOutput {
                    output_type: IOType::Bool,
                    index,
                }) => Ok(GraphBoolean::Variable(VariableIndex(index))),
                Ok(_) => Err(AnimGraphExpressionError::ExpectedBooleanRoute(
                    route.clone(),
                )),
                Err(err) => Err(AnimGraphExpressionError::InvalidBooleanRoute(err)),
            },
            Expression::Parameter(parameter) => {
                match self.get_or_create_parameter(&parameter.name, &IOType::Bool)? {
                    NodeCompilationOutput {
                        output_type: IOType::Bool,
                        index,
                    } => Ok(GraphBoolean::Variable(VariableIndex(index))),
                    _ => Err(AnimGraphExpressionError::ExpectedBooleanParameter(
                        parameter.name.clone(),
                    )),
                }
            }
            Expression::Query(Query::Event(query)) => {
                let query_state = match query.query {
                    QueryType::Exited => Some(FlowState::Exited),
                    QueryType::Entering => Some(FlowState::Entering),
                    QueryType::Entered => Some(FlowState::Entered),
                    QueryType::Exiting => Some(FlowState::Exiting),
                    QueryType::Active => None,
                };

                let event = self.get_or_create_event_index(&query.event.name)?;
                if let Some(in_state) = query_state {
                    Ok(GraphBoolean::QueryEvent(in_state, event))
                } else {
                    Ok(GraphBoolean::QueryEventActive(event))
                }
            }

            Expression::Query(Query::State(expr)) => {
                let query_state = match expr.query {
                    QueryType::Exited => Some(FlowState::Exited),
                    QueryType::Entering => Some(FlowState::Entering),
                    QueryType::Entered => Some(FlowState::Entered),
                    QueryType::Exiting => Some(FlowState::Exiting),
                    QueryType::Active => None,
                };
                if let Some(index) = graph.graph.index_of_state(&expr.state.name) {
                    if index <= graph.states.len() {
                        let state_index = index.try_state_index()?;
                        if let Some(in_state) = query_state {
                            return Ok(GraphBoolean::QueryState(in_state, state_index));
                        } else {
                            return Ok(GraphBoolean::QueryStateActive(state_index));
                        }
                    }
                }

                Err(AnimGraphExpressionError::InvalidStatePath(
                    expr.state.name.clone(),
                ))
            }
            Expression::Query(Query::StateMachine(expr)) => {
                let query_state = match expr.query {
                    QueryType::Exited => Some(FlowState::Exited),
                    QueryType::Entering => Some(FlowState::Entering),
                    QueryType::Entered => Some(FlowState::Entered),
                    QueryType::Exiting => Some(FlowState::Exiting),
                    QueryType::Active => None,
                };
                if let Some(index) = graph.graph.index_of_state_machine(&expr.state_machine.name) {
                    if index <= graph.machines.len() {
                        let state_machine = index.try_machine_index()?;
                        if let Some(in_state) = query_state {
                            return Ok(GraphBoolean::QueryMachineState(in_state, state_machine));
                        } else {
                            return Ok(GraphBoolean::QueryMachineActive(state_machine));
                        }
                    }
                }

                Err(AnimGraphExpressionError::InvalidStateMachine(
                    expr.state_machine.name.clone(),
                ))
            }
            Expression::CompilerGlobal(name) => {
                if let Some(index) = self.global_booleans.get(name) {
                    Ok(*index)
                } else {
                    Err(AnimGraphExpressionError::InvalidGlobalBoolean(name.clone()))
                }
            }

            Expression::Binary(..) | Expression::VectorProjection(..) => Err(
                AnimGraphExpressionError::InvalidBooleanExpression(expression.clone()),
            ),
            Expression::Not(_)
            | Expression::ContainsExclusive(..)
            | Expression::ContainsInclusive(..)
            | Expression::CompareBoolean(_)
            | Expression::CompareNumber(_)
            | Expression::Less(_)
            | Expression::Greater(_)
            | Expression::And(_)
            | Expression::Or(_)
            | Expression::Xor(_)
            | Expression::Debug { .. } => {
                if !allow_condition {
                    return Err(AnimGraphExpressionError::ConditionExpressionNotAllowed);
                }

                let index: IndexType = self.builder.desc.conditions.len().try_condition()?;

                self.builder.desc.conditions.push(GraphCondition::never());

                let condition = self
                    .resolve_condition_expression(expression, graph, context_path)?
                    .debug_break();

                self.builder.desc.conditions[index as usize] = condition;

                Ok(GraphBoolean::Condition(index))
            }
        }
    }

    fn resolve_expression(
        &mut self,
        expression: &Expression,
        io_type: &IOType,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
    ) -> Result<NodeCompilationInput, AnimGraphExpressionError> {
        let input = match io_type {
            IOType::Bool => NodeCompilationInput::Boolean(self.resolve_boolean_expression(
                expression,
                graph,
                context_path,
                true,
            )?),
            IOType::Number => NodeCompilationInput::Number(self.resolve_number_expression(
                expression,
                graph,
                context_path,
            )?),
            _ => {
                return Err(AnimGraphExpressionError::CannotResolveExpression(
                    io_type.clone(),
                    expression.clone(),
                ))
            }
        };

        Ok(input)
    }

    fn get_or_create_event_index(&mut self, name: &str) -> IndexedResult<Event> {
        if let Some(event) = self.events_lookup.get(name) {
            return Ok(*event);
        }

        let event = Event(self.create_output(name, IOType::Event)?.index);
        self.events_lookup.insert(name.to_owned(), event);
        Ok(event)
    }

    fn get_or_create_event(&mut self, name: &str) -> IndexedResult<NodeCompilationOutput> {
        let index = self.get_or_create_event_index(name)?;

        let result = NodeCompilationOutput {
            output_type: IOType::Event,
            index: index.0,
        };
        self.output_lookup.insert(result.clone(), name.to_owned());
        Ok(result)
    }

    fn get_or_create_parameter(
        &mut self,
        name: &str,
        io_type: &IOType,
    ) -> IndexedResult<NodeCompilationOutput> {
        if let Some(result) = self.parameters_lookup.get(name) {
            return Ok(result.clone());
        }

        let result = self.create_output(name, io_type.clone())?;
        self.output_lookup.insert(result.clone(), name.to_owned());
        self.parameters_lookup
            .insert(name.to_owned(), result.clone());

        let entry = match io_type {
            IOType::Bool => GraphParameterEntry::Boolean(result.index, Default::default()),
            IOType::Number => GraphParameterEntry::Number(result.index, Default::default()),
            IOType::Vector => GraphParameterEntry::Vector(result.index, Default::default()),
            IOType::Event => GraphParameterEntry::Event(result.index),
            IOType::Timer => GraphParameterEntry::Timer(result.index),
            IOType::Resource(_) => return Ok(result),
        };

        self.builder.parameters.insert(name.to_owned(), entry);

        Ok(result)
    }

    fn get_resource(
        &mut self,
        name: &str,
        io_type: &IOType,
        graph: &GraphCompilationContext,
        context: &ContextPath,
    ) -> Result<NodeCompilationOutput, CompileError> {
        if let Some(result) = self.resources_lookup.get(name) {
            return Ok(result.clone());
        }

        let Some(resource) = graph.graph.resources.iter().find(|x| x.name == name) else {
            return Err(CompileError::MissingResourceReference(
                io_type.to_str().to_owned(),
                name.to_owned(),
                context.inner().join(),
            ));
        };

        if resource.resource_type != io_type.to_str() {
            return Err(CompileError::UnexpectedResourceType(
                io_type.to_str().to_owned(),
                resource.resource_type.clone(),
                name.to_owned(),
                context.inner().join(),
            ));
        }

        let result = self.create_output(name, io_type.clone())?;
        self.builder.resources[result.index as usize].initial = Some(resource.content.clone());
        self.resources_lookup
            .insert(name.to_owned(), result.clone());
        self.output_lookup.insert(result.clone(), name.to_owned());
        Ok(result)
    }

    fn get_or_create_number_constant(
        &mut self,
        value: Number,
    ) -> IndexedResult<Option<GraphNumber>> {
        if let Some(result) = self.numbers_lookup.get(&value) {
            return Ok(Some(GraphNumber::Constant(*result)));
        }

        let number: f64 = if let Some(x) = value.as_f64() {
            if x == 0.0 {
                return Ok(Some(GraphNumber::Zero));
            }
            if x == 1.0 {
                return Ok(Some(GraphNumber::One));
            }
            x
        } else {
            return Ok(None);
        };

        let result = ConstantIndex(self.builder.constants.len().try_constant()?);
        self.builder.constants.push(number);
        self.numbers_lookup.insert(value, result);

        Ok(Some(GraphNumber::Constant(result)))
    }

    pub fn add_global_constant(&mut self, name: &str, value: f64) -> Result<(), CompileError> {
        let result = ConstantIndex(self.builder.constants.len().try_constant()?);
        self.builder.constants.push(value);

        if let Some(n) = Number::from_f64(value) {
            self.numbers_lookup.insert(n, result);
        }

        self.global_number
            .insert(name.to_owned(), GraphNumber::Constant(result));
        Ok(())
    }

    fn get_constant_vector(&self, list: &[Value]) -> Option<VectorRef> {
        match list {
            [Value::Number(x), Value::Number(y), Value::Number(z)] => Some(VectorRef::Constant([
                x.as_f64()? as f32,
                y.as_f64()? as f32,
                z.as_f64()? as f32,
            ])),
            _ => None,
        }
    }

    fn resolve_io(
        &mut self,
        io: &IO,
        io_type: &IOType,
        graph: &GraphCompilationContext,
        context_path: &ContextPath,
    ) -> Result<Option<NodeCompilationInput>, CompileError> {
        let output = match &io.value {
            IOSettings::Empty => return Ok(None),
            IOSettings::Event(event) => self.get_or_create_event(&event.event.name)?,
            IOSettings::Parameter(parameter) => {
                self.get_or_create_parameter(&parameter.parameter.name, io_type)?
            }
            IOSettings::Resource(resource) => {
                self.get_resource(&resource.resource.name, io_type, graph, context_path)?
            }
            IOSettings::Value(expr) => match &expr.value {
                Value::Null => return Ok(None),

                Value::Bool(x) if io_type == &IOType::Bool => {
                    if *x {
                        return Ok(Some(NodeCompilationInput::Boolean(GraphBoolean::Always)));
                    } else {
                        return Ok(Some(NodeCompilationInput::Boolean(GraphBoolean::Never)));
                    }
                }
                Value::Number(number) if io_type.is_number_type() => {
                    let result =
                        if let Some(result) = self.get_or_create_number_constant(number.clone())? {
                            result
                        } else {
                            return Err(AnimGraphNodeInputError::UnexpectedValue(
                                context_path.inner().with_slot(&io.name),
                                io_type.clone(),
                                expr.value.clone(),
                            )
                            .into());
                        };

                    return Ok(Some(NodeCompilationInput::Number(result)));
                }
                Value::Array(list) if io_type == &IOType::Vector && list.len() == 3 => {
                    match self.get_constant_vector(list) {
                        Some(res) => return Ok(Some(NodeCompilationInput::Vector(res))),
                        None => {
                            return Err(AnimGraphNodeInputError::UnexpectedValue(
                                context_path.inner().with_slot(&io.name),
                                io_type.clone(),
                                Value::Array(list.clone()),
                            )
                            .into())
                        }
                    }
                }

                value => {
                    return Err(AnimGraphNodeInputError::UnexpectedValue(
                        context_path.inner().with_slot(&io.name),
                        io_type.clone(),
                        value.clone(),
                    )
                    .into());
                }
            },

            IOSettings::Route(route) => match graph.resolve_route(context_path, &route.route) {
                Ok(output) => output,
                Err(err) => {
                    return Err(AnimGraphNodeInputError::RouteError(
                        context_path.inner().with_slot(&io.name),
                        io_type.clone(),
                        err,
                    )
                    .into());
                }
            },

            IOSettings::Expression(expression) => {
                if expression.expression == Expression::None {
                    return Ok(None);
                }

                match self.resolve_expression(&expression.expression, io_type, graph, context_path)
                {
                    Ok(input) => return Ok(Some(input)),
                    Err(err) => {
                        return Err(AnimGraphNodeInputError::ExpressionError(
                            context_path.inner().with_slot(&io.name),
                            io_type.clone(),
                            err,
                        )
                        .into())
                    }
                }
            }
        };

        if &output.output_type != io_type {
            return Err(AnimGraphNodeInputError::UnexpectedType(
                context_path.inner().with_slot(&io.name),
                io_type.clone(),
                output.output_type,
            )
            .into());
        }

        match io_type {
            IOType::Bool => Ok(Some(NodeCompilationInput::Boolean(GraphBoolean::Variable(
                VariableIndex(output.index),
            )))),
            IOType::Number => Ok(Some(NodeCompilationInput::Number(GraphNumber::Variable(
                VariableIndex(output.index),
            )))),
            IOType::Vector => Ok(Some(NodeCompilationInput::Vector(VectorRef::Variable(
                VariableIndex(output.index),
            )))),
            IOType::Event => Ok(Some(NodeCompilationInput::Event(Event(output.index)))),
            IOType::Timer => Ok(Some(NodeCompilationInput::Timer(Timer(output.index)))),
            IOType::Resource(resource_type) => {
                let type_index = self.get_or_create_resource_type(resource_type)?;
                Ok(Some(NodeCompilationInput::Resource {
                    type_index,
                    variable: VariableIndex(output.index),
                }))
            }
        }
    }

    fn create_default_input(
        &mut self,
        node: &NodeCompilationContext,
        slot_name: &str,
        io_type: &IOType,
        graph: &GraphCompilationContext,
    ) -> Result<NodeCompilationInput, CompileError> {
        match io_type {
            IOType::Bool => Ok(NodeCompilationInput::Boolean(GraphBoolean::Never)),
            IOType::Number => Ok(NodeCompilationInput::Number(GraphNumber::Zero)),
            IOType::Vector => Ok(NodeCompilationInput::Vector(VectorRef::default())),
            IOType::Event => {
                let full_name = node.path.with_slot(slot_name);
                let event = self.get_or_create_event_index(&full_name)?;
                Ok(NodeCompilationInput::Event(event))
            }
            IOType::Timer => {
                let full_name = node.path.with_slot(slot_name);
                let output = self.create_output(&full_name, IOType::Timer)?;
                self.output_lookup.insert(output.clone(), full_name);
                Ok(NodeCompilationInput::Timer(Timer(output.index)))
            }
            IOType::Resource(resource_type) => {
                let type_index = self.get_or_create_resource_type(resource_type)?;
                let name = format!("default_{resource_type}");
                let output =
                    self.get_resource(&name, io_type, graph, &ContextPath::Path(&node.path))?;

                Ok(NodeCompilationInput::Resource {
                    type_index,
                    variable: VariableIndex(output.index),
                })
            }
        }
    }

    fn build_inputs(&mut self, graph: &mut GraphCompilationContext) -> Result<(), CompileError> {
        let mut result = Vec::default();

        for node_index in 0..graph.nodes.len() {
            let graph_node = &graph.nodes[node_index];
            let inputs = graph_node.meta.input();
            if inputs.is_empty() {
                continue;
            }

            if let [(DEFAULT_INPUT_NAME, io_type)] = inputs {
                let mut input = if let Some(io) = graph_node
                    .node
                    .input
                    .iter()
                    .find(|x| x.name == DEFAULT_INPUT_NAME)
                {
                    self.resolve_io(io, io_type, graph, &ContextPath::ParentOf(&graph_node.path))?
                } else {
                    None
                };

                if input.is_none() {
                    input = Some(self.create_default_input(
                        &graph.nodes[node_index],
                        DEFAULT_INPUT_NAME,
                        io_type,
                        graph,
                    )?);
                }

                graph.nodes[node_index].default_input = input;
                continue;
            }

            let mut default_input = None;
            for (name, io_type) in inputs.iter().cloned() {
                let input = if let Some(io) = graph_node.node.input.iter().find(|x| x.name == name)
                {
                    self.resolve_io(
                        io,
                        &io_type,
                        graph,
                        &ContextPath::ParentOf(&graph_node.path),
                    )?
                } else {
                    None
                };

                let slot = match input {
                    Some(input) => input,
                    None => {
                        self.create_default_input(&graph.nodes[node_index], name, &io_type, graph)?
                    }
                };

                if name == DEFAULT_INPUT_NAME {
                    default_input = Some(slot.clone());
                }

                result.push(slot);
            }

            let node = &mut graph.nodes[node_index];
            node.default_input = default_input;
            node.inputs.append(&mut result);
        }

        Ok(())
    }

    fn build_parameter_initializers(
        &mut self,
        graph: &mut GraphCompilationContext,
    ) -> IndexedResult<()> {
        for parameter in graph.graph.parameters.iter() {
            let name = &parameter.name;
            let param = match &parameter.initial {
                &InitialParameterValue::Bool(value) => {
                    let index = self.get_or_create_parameter(name, &IOType::Bool)?.index;
                    GraphParameterEntry::Boolean(index, value)
                }
                &InitialParameterValue::Number(value) => {
                    let index = self.get_or_create_parameter(name, &IOType::Number)?.index;
                    GraphParameterEntry::Number(index, value)
                }
                &InitialParameterValue::Vector(value) => {
                    let index = self.get_or_create_parameter(name, &IOType::Vector)?.index;
                    GraphParameterEntry::Vector(index, value)
                }
                InitialParameterValue::Timer => {
                    let index = self.get_or_create_parameter(name, &IOType::Timer)?.index;
                    GraphParameterEntry::Timer(index)
                }
                InitialParameterValue::Event(target) => {
                    let result = self.get_or_create_event(&target.name)?;
                    self.parameters_lookup
                        .insert(name.to_owned(), result.clone());
                    GraphParameterEntry::Event(result.index)
                }
            };

            self.builder.parameters.insert(name.clone(), param);
        }
        Ok(())
    }

    fn build_outputs(&mut self, graph: &mut GraphCompilationContext) -> IndexedResult<()> {
        for node in graph.nodes.iter_mut() {
            let outputs = node.meta.output();

            if outputs.is_empty() {
                continue;
            }

            if let [(DEFAULT_OUPTUT_NAME, io_type)] = outputs {
                let name = node.path.join();

                let result = self.create_output(&name, io_type.clone())?;
                self.output_lookup.insert(result.clone(), name);
                node.default_output = Some(result);

                continue;
            }

            node.outputs.reserve(outputs.len());
            for (name, io_type) in outputs.iter().cloned() {
                let full_name = node.path.join();

                let output = self.create_output(&full_name, io_type)?;

                self.output_lookup.insert(output.clone(), full_name);

                if name == DEFAULT_OUPTUT_NAME {
                    node.default_output = Some(output.clone());
                }
                node.outputs.push(output);
            }
        }

        Ok(())
    }

    fn build_transitions(&mut self, graph: &GraphCompilationContext) -> Result<(), CompileError> {
        let count = graph.transitions.len().try_transition_index()?;

        self.builder
            .desc
            .transition_condition
            .reserve(count as usize);
        self.builder.desc.transitions.reserve(count as usize);
        for index in 0..count {
            let transition = &graph.transitions[index as usize];

            let condition = match self.resolve_condition_expression(
                &transition.transition.condition,
                graph,
                &ContextPath::Path(&transition.path),
            ) {
                Ok(condition) => condition,

                Err(err) => {
                    return Err(CompileError::TransitionConditionError(
                        transition.path.join(),
                        err,
                    ));
                }
            };

            self.builder.desc.transition_condition.push(condition);

            let blend = self
                .resolve_io(
                    &transition.transition.blend,
                    &IOType::Number,
                    graph,
                    &ContextPath::Path(&transition.path),
                )?
                .and_then(|x| x.as_number());
            let progress = self
                .resolve_io(
                    &transition.transition.progress,
                    &IOType::Number,
                    graph,
                    &ContextPath::Path(&transition.path),
                )?
                .and_then(|x| x.as_number());
            let duration = self
                .resolve_io(
                    &transition.transition.duration,
                    &IOType::Number,
                    graph,
                    &ContextPath::Path(&transition.path),
                )?
                .and_then(|x| x.as_number());

            let origin = transition.source_index;
            let target = transition.target_index;
            let immediate = transition.transition.immediate;
            let forceable = transition.transition.forceable;
            let node = transition.node;

            self.builder.desc.transitions.push(HierarchicalTransition {
                origin,
                target,
                forceable,
                immediate,
                blend,
                progress,
                duration,
                node,
            });
        }

        Ok(())
    }

    fn build_state_machines(
        &mut self,
        graph: &GraphCompilationContext,
    ) -> Result<(), CompileError> {
        self.builder.desc.machine_states =
            graph.machines.iter().map(|x| x.state_range.end.0).collect();
        self.builder.desc.state_transitions =
            graph.states.iter().map(|x| x.transitions.end).collect();
        self.builder.desc.state_branch = graph.states.iter().map(|x| x.branch_index).collect();
        self.builder.desc.state_global_condition = Vec::with_capacity(graph.states.len());
        for state in graph.states.iter() {
            let branch = graph
                .branches
                .get(state.branch_index as usize)
                .map(|x| x.get_branch_or_node_path(graph))
                .ok_or_else(|| CompileError::BranchIndexError)?;
            let global_condition = self
                .resolve_condition_expression(
                    &state.state.global_condition,
                    graph,
                    &ContextPath::Path(branch),
                )
                .map_err(|err| {
                    CompileError::GlobalTransitionConditionError(state.path.join(), err)
                })?;
            self.builder
                .desc
                .state_global_condition
                .push(global_condition);
        }

        self.builder.desc.branches = graph.branches.iter().map(|x| x.target).collect();
        Ok(())
    }
}

pub const BRANCH_SCOPE: &str = "branch";
pub const TRANSITION_SCOPE: &str = "transition";
pub const TREE_ROOT_SCOPE: &str = "node";

impl<'a> GraphCompilationContext<'a> {
    pub fn build_context(
        graph: &'a AnimGraph,
        registry: &'a NodeCompilationRegistry,
    ) -> Result<GraphCompilationContext<'a>, CompileError> {
        use CompileError::*;
        let mut paths = IndexedPath::default();

        let mut processed_machines = HashMap::default();

        let mut machines = Vec::default();
        let mut states = Vec::default();
        let mut transitions = Vec::default();
        let mut branches = Vec::default();
        let mut nodes = Vec::default();
        let mut aliases = HashMap::default();

        for (machine_route_index, state_machine) in graph.state_machines.iter().enumerate() {
            if state_machine.states.is_empty() {
                return Err(StateMachineHasNoStates(state_machine.name.clone()));
            }

            let machine_index = processed_machines.len().try_machine_index()?;
            debug_assert!(processed_machines
                .insert(state_machine.name.as_str(), machine_index)
                .is_none());

            paths.add_scope(Some(machine_route_index), state_machine.name.as_str());

            let first_state = states.len().try_state_index()?;
            let mut state_lookup = HashMap::with_capacity(state_machine.states.len());
            let start = branches.len();
            for (state_route_index, state) in state_machine.states.iter().enumerate() {
                paths.add_scope(Some(state_route_index), state.name.as_str());

                let global_index = branches.len().try_branch_index()?;
                branches.push(BranchCompilationContext {
                    path: paths.clone(),
                    branch: &state.branch,
                    target: HierarchicalBranch::default(),
                });

                let index = states.len().try_state_index()?;
                state_lookup.insert(state.name.clone(), index);
                states.push(StateCompilationContext {
                    path: paths.clone(),
                    state,
                    branch_index: global_index,
                    transitions: 0..0,
                });

                paths.remove_scope(Some(state_route_index), state.name.as_str());
            }
            let end = branches.len();

            for (state_route_index, (state, global_index)) in
                state_machine.states.iter().zip(start..end).enumerate()
            {
                paths.add_scope(Some(state_route_index), state.name.as_str());

                paths.add_scope(Some(0), BRANCH_SCOPE);
                let target = build_branch_scope(
                    &mut branches,
                    &mut paths,
                    &mut nodes,
                    &mut aliases,
                    &state.branch,
                    registry,
                    graph,
                    &mut processed_machines,
                )?;

                paths.remove_scope(Some(0), BRANCH_SCOPE);

                let context = branches.get_mut(global_index).expect("Valid");
                context.target = target;

                paths.remove_scope(Some(state_route_index), state.name.as_str());
            }

            let first_transition: IndexType = transitions.len().try_transition_index()?;
            for (state_route_index, (state, global_index)) in state_machine
                .states
                .iter()
                .zip(first_state.0 as usize..)
                .enumerate()
            {
                let start: IndexType = transitions.len().try_transition_index()?;
                let context = states.get_mut(global_index).expect("Valid");
                if state.transitions.is_empty() {
                    context.transitions = start..start;
                    continue;
                }
                paths.add_scope(Some(state_route_index), state.name.as_str());

                paths.add_scope(Some(1), TRANSITION_SCOPE);
                for (index, transition) in state.transitions.iter().enumerate() {
                    paths.add_scope(Some(index), &transition.target.name);
                    let node_index = if let Some(node) = transition.node.as_ref() {
                        Some(build_tree_root(
                            &mut paths,
                            &mut nodes,
                            &mut aliases,
                            node,
                            registry,
                        )?)
                    } else {
                        None
                    };

                    if let Some(target_state) = state_lookup.get(&transition.target.name) {
                        transitions.push(TransitionCompilationContext {
                            path: paths.clone(),
                            transition,
                            source_index: StateIndex(global_index as _),
                            target_index: *target_state,
                            node: node_index,
                        });
                    } else {
                        return Err(MissingTransitionTargetError(paths.join()));
                    }

                    paths.remove_scope(Some(index), &transition.target.name);
                }
                let end: IndexType = transitions.len().try_transition_index()?;
                context.transitions = start..end;

                paths.remove_scope(Some(1), TRANSITION_SCOPE);
                paths.remove_scope(Some(state_route_index), state.name.as_str());
            }
            let total_transition_range =
                first_transition..transitions.len().try_transition_index()?;
            let state_range = first_state..states.len().try_state_index()?;
            machines.push(StateMachineCompilationContext {
                state_machine,
                states: state_lookup,
                state_range,
                total_transition_range,
            });

            paths.remove_scope(Some(machine_route_index), state_machine.name.as_str());
        }

        Ok(GraphCompilationContext {
            graph,
            registry,
            machines,
            states,
            branches,
            nodes,
            aliases,
            transitions,
        })
    }
}

pub fn run_graph_definition_compilation(
    definition: &mut GraphDefinitionCompilation,
    graph: &mut GraphCompilationContext,
) -> Result<(), CompileError> {
    definition.build_parameter_initializers(graph)?;
    definition.build_outputs(graph)?;
    definition.build_inputs(graph)?;
    definition.build_transitions(graph)?;
    definition.build_state_machines(graph)?;

    let mut nodes = Vec::with_capacity(graph.nodes.len());

    definition.node_aliases = Vec::with_capacity(graph.nodes.len());

    let mut node_types = Vec::new();

    let mut type_lookup = HashMap::new();

    let metrics = {
        let mut metrics = definition.builder.metrics();
        metrics.nodes = graph.nodes.len();
        metrics
    };

    for node in graph.nodes.iter() {
        let type_index = if let Some(index) = type_lookup.get(&node.node.name) {
            *index
        } else {
            let index = node_types.len().try_node_type_index()?;
            node_types.push(node.node.name.clone());
            type_lookup.insert(node.node.name.clone(), index);
            index
        };
        let context = NodeSerializationContext {
            metrics: &metrics,
            node,
            resource_types: &definition.builder.resource_types,
        };

        nodes.push(GraphCompiledNode {
            type_id: type_index,
            value: node.meta.build(&context)?,
        });

        definition
            .node_aliases
            .push(if !node.node.alias.is_empty() {
                node.node.alias.clone()
            } else {
                node.node.name.clone()
            });
    }

    definition.builder.compiled_nodes = nodes;
    definition.builder.node_types = node_types;

    Ok(())
}
