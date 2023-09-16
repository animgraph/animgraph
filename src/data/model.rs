use std::{collections::HashMap, marker::PhantomData};

use glam::Vec3;
use serde_derive::{Deserialize, Serialize};

pub type Extras = Value;

pub use serde_json;
use serde_json::Number;
pub use serde_json::{to_value, Value};
use uuid::Uuid;

use crate::{
    io::{Event, Resource, Timer, VectorRef},
    Alpha, FromFloatUnchecked, NumberOperation, Projection, ResourceSettings, SampleTimer, Seconds,
};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AnimGraph {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub parameters: Vec<Parameter>,
    pub resources: Vec<ResourceContent>,
    pub resource_sets: Vec<ResourceSet>,
    pub state_machines: Vec<StateMachine>,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSet {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    #[serde(rename = "resource_set")]
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub resources: Vec<ResourceContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitialParameterValue {
    Bool(bool),
    Number(f64),
    Vector([f64; 3]),
    Timer,
    Event(UuidTarget),
}

impl Default for InitialParameterValue {
    fn default() -> Self {
        Self::Bool(false)
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub name: String,
    pub initial: InitialParameterValue,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

impl Parameter {
    pub fn new(name: String, initial: InitialParameterValue) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            name,
            initial,
            extras: Default::default(),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub name: String,
    pub resource_type: String,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub content: Value,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct StateMachine {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    #[serde(rename = "state_machine")]
    pub name: String,
    pub states: Vec<State>,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct State {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    #[serde(rename = "state")]
    pub name: String,
    pub global_condition: Expression,
    pub branch: Branch,
    pub transitions: Vec<Transition>,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub node: Option<Node>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target: Option<BranchTarget>,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchTarget {
    Postprocess(Box<Branch>),
    Layers(Vec<Branch>),
    StateMachine(UuidTarget),
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UuidTarget {
    pub name: String,
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub uuid: Uuid,
}

impl UuidTarget {
    pub const fn new() -> Self {
        Self {
            name: String::new(),
            uuid: Uuid::nil(),
        }
    }

    pub fn clear(&mut self) {
        self.name.clear();
        self.uuid = Uuid::nil();
    }
}

impl From<&str> for UuidTarget {
    fn from(value: &str) -> Self {
        Self {
            name: value.to_owned(),
            uuid: Uuid::nil(),
        }
    }
}

impl From<String> for UuidTarget {
    fn from(value: String) -> Self {
        Self {
            name: value,
            uuid: Uuid::nil(),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub target: UuidTarget,
    pub node: Option<Node>,
    pub progress: IO,
    pub blend: IO,
    pub duration: IO,
    pub condition: Expression,
    pub forceable: bool,
    pub immediate: bool,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub name: String,
    pub extras: Extras,
}

impl From<&str> for Output {
    fn from(value: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: value.to_owned(),
            ..Default::default()
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub alias: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input: Vec<IO>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output: Vec<Output>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<Node>,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub settings: Extras,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct IO {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub name: String,
    #[serde(skip_serializing_if = "IOSettings::is_empty", default)]
    pub value: IOSettings,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

pub const DEFAULT_INPUT_NAME: &str = "";
pub const DEFAULT_OUPTUT_NAME: &str = "";

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum IOSettings {
    #[default]
    Empty,
    Event(IOEventSettings),
    Route(IORouteSettings),
    Parameter(IOParameterSettings),
    Resource(IOResourceSettings),
    Value(IOValueSettings),
    Expression(IOExpressionSettings),
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IOEventSettings {
    pub event: UuidTarget,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IORouteSettings {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub route: String,
    pub target: UuidTarget,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IOParameterSettings {
    pub parameter: UuidTarget,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IOResourceSettings {
    pub resource: UuidTarget,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IOValueSettings {
    pub value: Value,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IOExpressionSettings {
    #[serde(skip_serializing_if = "Uuid::is_nil", default)]
    pub id: Uuid,
    pub expression: Expression,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub extras: Extras,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[must_use]
pub enum Expression {
    #[default]
    None,
    Value(Value),
    Route(IORouteSettings),
    Parameter(UuidTarget),
    Query(Query),
    CompilerGlobal(String),
    ContainsExclusive(Box<(Expression, Expression, Expression)>),
    ContainsInclusive(Box<(Expression, Expression, Expression)>),
    VectorProjection(Projection, Box<Expression>),
    Not(Box<Expression>),
    CompareBoolean(Box<(Expression, Expression)>),
    CompareNumber(Box<(Expression, Expression)>),
    Binary(NumberOperation, Box<(Expression, Expression)>),

    Less(Box<(Expression, Expression)>),
    Greater(Box<(Expression, Expression)>),

    And(Vec<Expression>),
    Or(Vec<Expression>),
    Xor(Vec<Expression>),

    Debug {
        trigger: Value,
        condition: Box<Expression>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Query {
    Event(EventQuery),
    State(StateQuery),
    StateMachine(StateMachineQuery),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    Exited,
    Entering,
    Entered,
    Exiting,
    Active, // Entering || Entered || Exiting
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateMachineQuery {
    pub state_machine: UuidTarget,
    pub query: QueryType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateQuery {
    pub state: UuidTarget,
    pub query: QueryType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventQuery {
    pub event: UuidTarget,
    pub query: QueryType,
}

#[derive(Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UuidContext {
    pub machines: HashMap<String, Uuid>,
    pub states: HashMap<String, Uuid>,
    pub parameters: HashMap<String, Uuid>,
    pub resources: HashMap<String, Uuid>,
    pub events: HashMap<String, Uuid>,
}

impl UuidContext {
    fn check_event(&mut self, target: &mut UuidTarget) {
        if target.uuid.is_nil() {
            if let Some(uuid) = self.events.get(&target.name) {
                target.uuid = *uuid;
            } else {
                target.uuid = Uuid::new_v4();
            }
        }
        self.events.insert(target.name.clone(), target.uuid);
    }

    fn check_parameter(&mut self, target: &mut UuidTarget) {
        if target.uuid.is_nil() {
            if let Some(uuid) = self.parameters.get(&target.name) {
                target.uuid = *uuid;
            } else {
                target.uuid = Uuid::new_v4();
            }
        }
        self.parameters.insert(target.name.clone(), target.uuid);
    }

    fn check_resource(&mut self, target: &mut UuidTarget) {
        if target.uuid.is_nil() {
            if let Some(uuid) = self.resources.get(&target.name) {
                target.uuid = *uuid;
            } else {
                target.uuid = Uuid::new_v4();
            }
        }
        self.resources.insert(target.name.clone(), target.uuid);
    }
}

impl AnimGraph {
    pub fn generate_id_for_nils(&mut self) {
        fn check_id(id: &mut Uuid) {
            if id.is_nil() {
                *id = Uuid::new_v4();
            }
        }

        let mut context = UuidContext::default();
        check_id(&mut self.id);
        for parameter in self.parameters.iter_mut() {
            check_id(&mut parameter.id);
            context
                .parameters
                .insert(parameter.name.clone(), parameter.id);

            match &mut parameter.initial {
                InitialParameterValue::Bool(_) => {}
                InitialParameterValue::Number(_) => {}
                InitialParameterValue::Vector(_) => {}
                InitialParameterValue::Timer => {}
                InitialParameterValue::Event(event) => context.check_event(event),
            }
        }

        for resource in self.resources.iter_mut() {
            check_id(&mut resource.id);
            context.resources.insert(resource.name.clone(), resource.id);
        }

        for set in self.resource_sets.iter_mut() {
            check_id(&mut set.id);

            for parameter in set.parameters.iter_mut() {
                if let Some(uuid) = context.parameters.get(&parameter.name) {
                    parameter.id = *uuid;
                } else {
                    check_id(&mut parameter.id);
                }

                match &mut parameter.initial {
                    InitialParameterValue::Bool(_) => {}
                    InitialParameterValue::Number(_) => {}
                    InitialParameterValue::Vector(_) => {}
                    InitialParameterValue::Timer => {}
                    InitialParameterValue::Event(event) => context.check_event(event),
                }
            }

            for resource in set.resources.iter_mut() {
                if let Some(uuid) = context.resources.get(&resource.name) {
                    resource.id = *uuid;
                } else {
                    check_id(&mut resource.id);
                }
            }
        }

        fn check_node(node: &mut Node, context: &mut UuidContext) {
            node.visit_nodes_mut(&mut |node| {
                check_id(&mut node.id);

                for io in node.input.iter_mut() {
                    check_io(io, context);
                }
            })
        }

        fn check_expr(expr: &mut Expression, context: &mut UuidContext) {
            expr.visit_mut(&mut |expr| match expr {
                Expression::None => {}
                Expression::Value(_) => {}
                Expression::Route(route) => {
                    check_id(&mut route.id);
                }
                Expression::Parameter(parameter) => {
                    context.check_parameter(parameter);
                }
                Expression::Query(q) => match q {
                    Query::Event(query) => {
                        context.check_event(&mut query.event);
                    }
                    Query::State(query) => {
                        if let Some(&id) = &context.states.get(&query.state.name) {
                            query.state.uuid = id;
                        }
                    }
                    Query::StateMachine(query) => {
                        if let Some(&id) = &context.machines.get(&query.state_machine.name) {
                            query.state_machine.uuid = id;
                        }
                    }
                },
                Expression::Binary(..) => {}
                Expression::CompilerGlobal(_) => {}
                Expression::ContainsExclusive(_) => {}
                Expression::ContainsInclusive(_) => {}
                Expression::VectorProjection(_, _) => {}
                Expression::Not(_) => {}
                Expression::CompareBoolean(_) => {}
                Expression::CompareNumber(_) => {}
                Expression::Less(_) => {}
                Expression::Greater(_) => {}
                Expression::And(_) => {}
                Expression::Or(_) => {}
                Expression::Xor(_) => {}
                Expression::Debug { .. } => {}
            });
        }

        fn check_io(io: &mut IO, context: &mut UuidContext) {
            check_id(&mut io.id);

            match &mut io.value {
                IOSettings::Empty => {}
                IOSettings::Event(settings) => {
                    context.check_event(&mut settings.event);
                }
                IOSettings::Route(settings) => {
                    check_id(&mut settings.id);
                }
                IOSettings::Parameter(settings) => {
                    context.check_parameter(&mut settings.parameter);
                }
                IOSettings::Resource(settings) => {
                    context.check_resource(&mut settings.resource);
                }
                IOSettings::Value(_settings) => {}
                IOSettings::Expression(settings) => {
                    check_expr(&mut settings.expression, context);
                }
            }
        }

        for machine in self.state_machines.iter_mut() {
            check_id(&mut machine.id);
            context.machines.insert(machine.name.clone(), machine.id);
        }

        for machine in self.state_machines.iter_mut() {
            context.states.clear();
            for state in machine.states.iter_mut() {
                check_id(&mut state.id);
                context.states.insert(state.name.clone(), state.id);
            }

            for state in machine.states.iter_mut() {
                state.branch.visit_branches_mut(&mut |branch| {
                    check_id(&mut branch.id);

                    if let Some(node) = &mut branch.node {
                        check_node(node, &mut context);
                    }

                    if let Some(target) = &mut branch.target {
                        match target {
                            BranchTarget::Postprocess(_) => {}
                            BranchTarget::Layers(_) => {}
                            BranchTarget::StateMachine(target) => {
                                if let Some(id) = context.machines.get(&target.name) {
                                    target.uuid = *id;
                                }
                            }
                        }
                    }
                });

                check_expr(&mut state.global_condition, &mut context);
                for transition in state.transitions.iter_mut() {
                    check_id(&mut transition.id);

                    if let Some(id) = context.states.get(&transition.target.name) {
                        transition.target.uuid = *id;
                    }

                    if let Some(node) = &mut transition.node {
                        check_node(node, &mut context);
                    }

                    check_expr(&mut transition.condition, &mut context);
                    check_io(&mut transition.blend, &mut context);
                    check_io(&mut transition.progress, &mut context);
                    check_io(&mut transition.duration, &mut context);
                }
            }
        }
    }

    pub fn find_state_machine<'a>(&'a self, name: &str) -> Option<&'a StateMachine> {
        self.state_machines.iter().find(|x| x.name == name)
    }

    pub fn find_state_machine_mut<'a>(&'a mut self, name: &str) -> Option<&'a mut StateMachine> {
        self.state_machines.iter_mut().find(|x| x.name == name)
    }

    pub fn index_of_state_machine(&self, name: &str) -> Option<usize> {
        self.state_machines.iter().position(|x| x.name == name)
    }

    pub fn index_of_state(&self, name: &str) -> Option<usize> {
        let (state_machine, state) = name.strip_prefix("::").unwrap_or(name).split_once("::")?;

        let state_machine_index = self.index_of_state_machine(state_machine)?;

        let start = self
            .state_machines
            .iter()
            .map(|x| x.states.len())
            .take(state_machine_index)
            .sum::<usize>();

        self.state_machines[state_machine_index]
            .states
            .iter()
            .position(|x| x.name == state)
            .map(|x| x + start)
    }

    pub fn visit_nodes_mut(&mut self, visitor: &mut impl FnMut(&mut Node)) {
        fn visit_node_mut(node: &mut Node, visitor: &mut impl FnMut(&mut Node)) {
            visitor(node);

            for child in node.children.iter_mut() {
                visit_node_mut(child, visitor);
            }
        }

        fn branch_visit_node_mut(branch: &mut Branch, visitor: &mut impl FnMut(&mut Node)) {
            if let Some(node) = branch.node.as_mut() {
                visit_node_mut(node, visitor);
            }

            if let Some(target) = branch.target.as_mut() {
                match target {
                    BranchTarget::Postprocess(branch) => {
                        branch_visit_node_mut(branch, visitor);
                    }
                    BranchTarget::Layers(layers) => {
                        for branch in layers.iter_mut() {
                            branch_visit_node_mut(branch, visitor);
                        }
                    }
                    BranchTarget::StateMachine { .. } => {}
                }
            }
        }

        for machine in self.state_machines.iter_mut() {
            for state in machine.states.iter_mut() {
                branch_visit_node_mut(&mut state.branch, visitor);

                for transition in state.transitions.iter_mut() {
                    if let Some(node) = transition.node.as_mut() {
                        visit_node_mut(node, visitor);
                    }
                }
            }
        }
    }

    pub fn visit_nodes(self, visitor: &mut impl FnMut(&Node)) {
        fn visit_node(node: &Node, visitor: &mut impl FnMut(&Node)) {
            visitor(node);

            for child in node.children.iter() {
                visit_node(child, visitor);
            }
        }

        fn branch_visit_node(branch: &Branch, visitor: &mut impl FnMut(&Node)) {
            if let Some(node) = branch.node.as_ref() {
                visit_node(node, visitor);
            }

            if let Some(target) = branch.target.as_ref() {
                match target {
                    BranchTarget::Postprocess(branch) => {
                        branch_visit_node(branch, visitor);
                    }
                    BranchTarget::Layers(layers) => {
                        for branch in layers.iter() {
                            branch_visit_node(branch, visitor);
                        }
                    }
                    BranchTarget::StateMachine { .. } => {}
                }
            }
        }

        for machine in self.state_machines.iter() {
            for state in machine.states.iter() {
                branch_visit_node(&state.branch, visitor);

                for transition in state.transitions.iter() {
                    if let Some(node) = transition.node.as_ref() {
                        visit_node(node, visitor);
                    }
                }
            }
        }
    }
}

impl State {
    pub fn global_name(&self, machine: &StateMachine) -> String {
        format!("{}::{}", machine.name, self.name)
    }

    #[must_use]
    pub fn with_machine_branch(mut self, target: &str) -> Self {
        self.branch.target = Some(BranchTarget::StateMachine(target.into()));
        self
    }

    #[must_use]
    pub fn with_layers(mut self, layers: impl Into<Vec<Branch>>) -> Self {
        self.branch.target = Some(BranchTarget::Layers(layers.into()));
        self
    }

    #[must_use]
    pub fn with_transitions(mut self, transitions: impl Into<Vec<Transition>>) -> Self {
        self.transitions = transitions.into();
        self
    }

    #[must_use]
    pub fn with_branch(mut self, branch: Branch) -> Self {
        self.branch = branch;
        self
    }

    #[must_use]
    pub fn with_global_condition(mut self, condition: impl Into<Expression>) -> Self {
        self.global_condition = condition.into();
        self
    }

    #[must_use]
    pub fn with(mut self, branch: Branch, transitions: impl Into<Vec<Transition>>) -> Self {
        self.branch = branch;
        self.transitions = transitions.into();
        self
    }
}

impl Branch {
    pub fn visit_branches(&self, visitor: &mut impl FnMut(&Branch)) {
        fn branch_visit(branch: &Branch, visitor: &mut impl FnMut(&Branch)) {
            visitor(branch);

            if let Some(target) = branch.target.as_ref() {
                match target {
                    BranchTarget::Postprocess(branch) => {
                        branch_visit(branch, visitor);
                    }
                    BranchTarget::Layers(layers) => {
                        for branch in layers.iter() {
                            branch_visit(branch, visitor);
                        }
                    }
                    BranchTarget::StateMachine { .. } => {}
                }
            }
        }

        branch_visit(self, visitor);
    }

    pub fn visit_branches_mut(&mut self, visitor: &mut impl FnMut(&mut Branch)) {
        fn branch_visit(branch: &mut Branch, visitor: &mut impl FnMut(&mut Branch)) {
            visitor(branch);

            if let Some(target) = branch.target.as_mut() {
                match target {
                    BranchTarget::Postprocess(branch) => {
                        branch_visit(branch, visitor);
                    }
                    BranchTarget::Layers(layers) => {
                        for branch in layers.iter_mut() {
                            branch_visit(branch, visitor);
                        }
                    }
                    BranchTarget::StateMachine { .. } => {}
                }
            }
        }

        branch_visit(self, visitor);
    }
}

impl Transition {
    pub fn with_duration(mut self, duration: impl IOSlot<Seconds>) -> Self {
        self.duration = duration.into_slot("duration");
        self
    }

    pub fn with_blend(mut self, blend: impl IOSlot<Alpha>) -> Self {
        self.blend = blend.into_slot("blend");
        self
    }

    pub fn with_progress(mut self, progress: impl IOSlot<Alpha>) -> Self {
        self.progress = progress.into_slot("progress");
        self
    }

    pub fn with_debug_trigger(mut self, value: impl Into<Value>) -> Self {
        self.condition = self.condition.debug(value.into());
        self
    }
}

impl Expression {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn visit(&self, visitor: &mut impl FnMut(&Expression)) {
        use Expression::*;
        visitor(self);
        match self {
            None => {}
            Value(_) => {}
            Route(_) => {}
            Parameter(_) => {}
            Query(_) => {}
            CompilerGlobal(_) => {}
            ContainsExclusive(e) | ContainsInclusive(e) => {
                e.0.visit(visitor);
                e.1.visit(visitor);
                e.2.visit(visitor);
            }
            Not(e) | VectorProjection(_, e) => {
                e.visit(visitor);
            }
            Binary(_, e) | CompareBoolean(e) | CompareNumber(e) | Less(e) | Greater(e) => {
                e.0.visit(visitor);
                e.1.visit(visitor);
            }
            And(list) | Or(list) | Xor(list) => {
                for e in list {
                    e.visit(visitor);
                }
            }
            Debug {
                trigger: _,
                condition,
            } => condition.visit(visitor),
        }
    }

    pub fn visit_mut(&mut self, visitor: &mut impl FnMut(&mut Expression)) {
        use Expression::*;
        visitor(self);
        match self {
            None => {}
            Value(_) => {}
            Route(_) => {}
            Parameter(_) => {}
            Query(_) => {}
            CompilerGlobal(_) => {}
            ContainsExclusive(e) | ContainsInclusive(e) => {
                e.0.visit_mut(visitor);
                e.1.visit_mut(visitor);
                e.2.visit_mut(visitor);
            }
            Not(e) | VectorProjection(_, e) => {
                e.visit_mut(visitor);
            }
            Binary(_, e) | CompareBoolean(e) | CompareNumber(e) | Less(e) | Greater(e) => {
                e.0.visit_mut(visitor);
                e.1.visit_mut(visitor);
            }
            And(list) | Or(list) | Xor(list) => {
                for e in list {
                    e.visit_mut(visitor);
                }
            }
            Debug {
                trigger: _,
                condition,
            } => condition.visit_mut(visitor),
        }
    }
}

impl Node {
    pub fn new<T: NodeSettings>(
        input: impl Into<Vec<IO>>,
        children: impl Into<Vec<Node>>,
        settings: T,
    ) -> anyhow::Result<Node> {
        Ok(Self {
            name: T::name().to_owned(),
            input: input.into(),
            children: children.into(),
            settings: settings.build()?,
            ..Default::default()
        })
    }

    pub fn new_disconnected<T: NodeSettings>(settings: T) -> anyhow::Result<Node> {
        Ok(Self {
            name: T::name().to_owned(),
            settings: settings.build()?,
            ..Default::default()
        })
    }

    pub fn with_alias(mut self, alias: &str) -> Self {
        self.alias = alias.to_owned();
        self
    }

    pub fn visit_nodes(&self, visitor: &mut impl FnMut(&Node)) {
        fn visit_node(node: &Node, visitor: &mut impl FnMut(&Node)) {
            visitor(node);
            for child in node.children.iter() {
                visit_node(child, visitor);
            }
        }

        visit_node(self, visitor);
    }

    pub fn visit_nodes_mut(&mut self, visitor: &mut impl FnMut(&mut Node)) {
        fn visit_node_mut(node: &mut Node, visitor: &mut impl FnMut(&mut Node)) {
            visitor(node);
            for child in node.children.iter_mut() {
                visit_node_mut(child, visitor);
            }
        }

        visit_node_mut(self, visitor);
    }
}

impl IO {
    pub fn new(name: &str, value: IOSettings) -> Self {
        Self {
            name: name.to_owned(),
            value,
            ..Default::default()
        }
    }
}

impl IOSettings {
    pub fn is_empty(&self) -> bool {
        self == &Self::Empty
    }

    pub fn is_route(&self) -> bool {
        matches!(self, Self::Route(_))
    }

    pub fn is_expression(&self) -> bool {
        matches!(self, Self::Expression(_))
    }

    pub fn from_f32(value: f32) -> Self {
        if value == f32::default() {
            return Self::Empty;
        }

        if let Some(value) = Number::from_f64(value as _).map(Value::Number) {
            Self::Value(IOValueSettings {
                value,
                ..Default::default()
            })
        } else {
            Self::Empty
        }
    }

    pub fn from_f64(value: f64) -> Self {
        if value == f64::default() {
            return Self::Empty;
        }

        if let Some(value) = Number::from_f64(value as _).map(Value::Number) {
            Self::Value(IOValueSettings {
                value,
                ..Default::default()
            })
        } else {
            Self::Empty
        }
    }

    pub fn from_bool(value: bool) -> Self {
        if value == bool::default() {
            Self::Empty
        } else {
            Self::Value(IOValueSettings {
                value: Value::Bool(value),
                ..Default::default()
            })
        }
    }

    pub fn from_vector32(value: [f32; 3]) -> Self {
        if value == [0.0, 0.0, 0.0] {
            return Self::Empty;
        }

        let zero = Number::from_f64(0.0).unwrap();
        let x = Number::from_f64(value[0] as _).unwrap_or_else(|| zero.clone());
        let y = Number::from_f64(value[1] as _).unwrap_or_else(|| zero.clone());
        let z = Number::from_f64(value[2] as _).unwrap_or_else(|| zero.clone());
        Self::Value(IOValueSettings {
            value: Value::Array(vec![Value::Number(x), Value::Number(y), Value::Number(z)]),
            ..Default::default()
        })
    }

    pub fn from_vector64(value: [f64; 3]) -> Self {
        if value == [0.0, 0.0, 0.0] {
            return Self::Empty;
        }

        let zero = Number::from_f64(0.0).unwrap();
        let x = Number::from_f64(value[0] as _).unwrap_or_else(|| zero.clone());
        let y = Number::from_f64(value[1] as _).unwrap_or_else(|| zero.clone());
        let z = Number::from_f64(value[2] as _).unwrap_or_else(|| zero.clone());
        Self::Value(IOValueSettings {
            value: Value::Array(vec![Value::Number(x), Value::Number(y), Value::Number(z)]),
            ..Default::default()
        })
    }

    pub fn from_timer(value: SampleTimer) -> Self {
        if value == SampleTimer::FIXED {
            return Self::Empty;
        }

        if let Ok(value) = serde_json::to_value(value) {
            Self::Value(IOValueSettings {
                value,
                ..Default::default()
            })
        } else {
            Self::Empty
        }
    }

    pub fn from_value(value: Value) -> Self {
        Self::Value(IOValueSettings {
            value,
            ..Default::default()
        })
    }

    pub fn from_event(name: String) -> Self {
        Self::Event(IOEventSettings {
            event: name.into(),
            ..Default::default()
        })
    }

    pub fn from_resource(name: String) -> Self {
        Self::Resource(IOResourceSettings {
            resource: name.into(),
            ..Default::default()
        })
    }

    pub fn from_route(name: String) -> Self {
        Self::Route(IORouteSettings {
            route: name,
            ..Default::default()
        })
    }

    pub fn from_parameter(name: String) -> Self {
        Self::Parameter(IOParameterSettings {
            parameter: name.into(),
            ..Default::default()
        })
    }

    pub fn from_expression(expression: Expression) -> Self {
        Self::Expression(IOExpressionSettings {
            expression,
            ..Default::default()
        })
    }
}

#[allow(clippy::should_implement_trait)]
impl Expression {
    pub fn debug(self, trigger: Value) -> Self {
        Self::Debug {
            trigger,
            condition: Box::new(self),
        }
    }

    pub fn not(self) -> Self {
        Self::Not(Box::new(self))
    }

    pub fn compare_boolean(self, other: impl Into<Self>) -> Self {
        Self::CompareBoolean(Box::new((self, other.into())))
    }

    pub fn compare_number(self, other: impl Into<Self>) -> Self {
        Self::CompareNumber(Box::new((self, other.into())))
    }

    pub fn lt(self, other: impl Into<Self>) -> Self {
        Self::Less(Box::new((self, other.into())))
    }

    pub fn ge(self, other: impl Into<Self>) -> Self {
        self.lt(other).not()
    }

    pub fn gt(self, other: impl Into<Self>) -> Self {
        Self::Greater(Box::new((self, other.into())))
    }

    pub fn le(self, other: impl Into<Self>) -> Self {
        self.gt(other).not()
    }

    pub fn add(self, other: impl Into<Self>) -> Self {
        Self::Binary(NumberOperation::Add, Box::new((self, other.into())))
    }

    pub fn subtract(self, other: impl Into<Self>) -> Self {
        Self::Binary(NumberOperation::Subtract, Box::new((self, other.into())))
    }

    pub fn divide(self, other: impl Into<Self>) -> Self {
        Self::Binary(NumberOperation::Divide, Box::new((self, other.into())))
    }

    pub fn modulus(self, other: impl Into<Self>) -> Self {
        Self::Binary(NumberOperation::Modulus, Box::new((self, other.into())))
    }

    pub fn multiply(self, other: impl Into<Self>) -> Self {
        Self::Binary(NumberOperation::Multiply, Box::new((self, other.into())))
    }

    pub fn or(self, other: impl Into<Self>) -> Self {
        match (self, other.into()) {
            (Self::Or(x), Self::Or(y)) => Self::Or([x, y].concat()),
            (Self::Or(mut x), y) => {
                x.push(y);
                Self::Or(x)
            }
            (y, Self::Or(mut x)) => {
                x.push(y);
                Self::Or(x)
            }
            (lhs, rhs) => Self::Or(vec![lhs, rhs]),
        }
    }

    pub fn xor(self, other: impl Into<Self>) -> Self {
        match (self, other.into()) {
            (Self::Xor(x), Self::Xor(y)) => Self::Xor([x, y].concat()),
            (Self::Xor(mut x), y) => {
                x.push(y);
                Self::Xor(x)
            }
            (y, Self::Xor(mut x)) => {
                x.push(y);
                Self::Xor(x)
            }
            (lhs, rhs) => Self::Xor(vec![lhs, rhs]),
        }
    }

    pub fn and(self, other: impl Into<Self>) -> Self {
        match (self, other.into()) {
            (Self::And(x), Self::And(y)) => Self::And([x, y].concat()),
            (Self::And(mut x), y) => {
                x.push(y);
                Self::And(x)
            }
            (y, Self::And(mut x)) => {
                x.push(y);
                Self::And(x)
            }
            (lhs, rhs) => Self::And(vec![lhs, rhs]),
        }
    }

    pub fn transition(self, target: &str, duration: Seconds) -> Transition {
        Transition {
            target: target.into(),
            condition: self,
            ..Default::default()
        }
        .with_duration(duration)
    }

    pub fn immediate_transition(self, target: &str) -> Transition {
        Transition {
            target: target.into(),
            condition: self,
            immediate: true,
            ..Default::default()
        }
    }
}

pub trait BooleanExpression: Sized + Into<Expression> {
    fn into_expr(self) -> Expression {
        self.into()
    }

    fn not(self) -> Expression {
        self.into_expr().not()
    }

    fn equals(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().compare_boolean(other)
    }

    fn not_equal(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().compare_boolean(other).not()
    }

    fn or(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().or(other)
    }

    fn xor(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().xor(other)
    }

    fn and(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().and(other)
    }

    fn transition(self, target: &str, duration: Seconds) -> Transition {
        Transition {
            target: target.into(),
            condition: self.into_expr(),
            ..Default::default()
        }
        .with_duration(duration)
    }
}

pub trait NumberExpression: Sized + Into<Expression> {
    fn into_expr(self) -> Expression {
        self.into()
    }

    fn equals(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().compare_number(other)
    }

    fn not_equal(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().compare_number(other).not()
    }

    fn lt(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().lt(other)
    }

    fn ge(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().ge(other)
    }

    fn gt(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().gt(other)
    }

    fn le(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().le(other)
    }

    fn add(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().add(other)
    }

    fn subtract(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().subtract(other)
    }

    fn multiply(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().multiply(other)
    }

    fn divide(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().divide(other)
    }

    fn modulus(self, other: impl Into<Expression>) -> Expression {
        self.into_expr().modulus(other)
    }
}

pub fn contains_exclusive(
    range: (impl Into<Expression>, impl Into<Expression>),
    value: impl Into<Expression>,
) -> Expression {
    Expression::ContainsExclusive(Box::new((range.0.into(), range.1.into(), value.into())))
}

pub fn contains_inclusive(
    range: (impl Into<Expression>, impl Into<Expression>),
    value: impl Into<Expression>,
) -> Expression {
    Expression::ContainsInclusive(Box::new((range.0.into(), range.1.into(), value.into())))
}

pub trait VectorExpression {
    fn projection(self, projection: Projection) -> Expression;
}

impl From<[f32; 3]> for Expression {
    fn from(value: [f32; 3]) -> Self {
        let zero = Number::from_f64(0.0).unwrap();
        let x = Number::from_f64(value[0] as _).unwrap_or_else(|| zero.clone());
        let y = Number::from_f64(value[1] as _).unwrap_or_else(|| zero.clone());
        let z = Number::from_f64(value[2] as _).unwrap_or_else(|| zero.clone());

        Self::Value(Value::Array(vec![
            Value::Number(x),
            Value::Number(y),
            Value::Number(z),
        ]))
    }
}

impl VectorExpression for [f32; 3] {
    fn projection(self, projection: Projection) -> Expression {
        Expression::VectorProjection(projection, Box::new(self.into()))
    }
}

impl VectorExpression for BindParameter<[f32; 3]> {
    fn projection(self, projection: Projection) -> Expression {
        Expression::VectorProjection(projection, Box::new(Expression::Parameter(self.0.into())))
    }
}

impl VectorExpression for BindRoute<[f32; 3]> {
    fn projection(self, projection: Projection) -> Expression {
        Expression::VectorProjection(
            projection,
            Box::new(Expression::Route(IORouteSettings {
                route: self.0,
                ..Default::default()
            })),
        )
    }
}

impl VectorExpression for BindParameter<Vec3> {
    fn projection(self, projection: Projection) -> Expression {
        Expression::VectorProjection(projection, Box::new(Expression::Parameter(self.0.into())))
    }
}

impl VectorExpression for BindRoute<Vec3> {
    fn projection(self, projection: Projection) -> Expression {
        Expression::VectorProjection(
            projection,
            Box::new(Expression::Route(IORouteSettings {
                route: self.0,
                ..Default::default()
            })),
        )
    }
}

impl BooleanExpression for Query {}

impl From<bool> for InitialParameterValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl<T: FromFloatUnchecked> From<T> for InitialParameterValue {
    fn from(value: T) -> Self {
        Self::Number(value.into_f64())
    }
}

// Marker struct. See `bind_route`
#[derive(Debug, Clone)]
pub struct BindRoute<T>(pub String, PhantomData<T>);

pub fn bind_route<T>(route: &str) -> BindRoute<T> {
    BindRoute(route.to_owned(), PhantomData)
}

impl BooleanExpression for BindRoute<bool> {}
impl<T: FromFloatUnchecked> NumberExpression for BindRoute<T> {}

// Marker struct. See `bind_parameter`
#[must_use]
#[derive(Debug, Clone)]
pub struct BindParameter<T>(pub String, PhantomData<T>);

pub fn bind_parameter<T>(name: &str) -> BindParameter<T> {
    BindParameter(name.to_owned(), PhantomData)
}

impl BooleanExpression for BindParameter<bool> {}
impl<T: FromFloatUnchecked> NumberExpression for BindParameter<T> {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IOType {
    Bool,
    Number,
    Vector,
    Event,
    Timer,
    Resource(&'static str),
}

impl IOType {
    pub fn is_number_type(&self) -> bool {
        matches!(self, Self::Number)
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            IOType::Bool => "bool",
            IOType::Number => "number",
            IOType::Vector => "vector",
            IOType::Event => "event",
            IOType::Timer => "timer",
            &IOType::Resource(res) => res,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum NodeChildRange {
    Exactly(usize),
    UpTo(usize),
}

impl NodeChildRange {
    pub fn has_room_for_more(&self, current: usize) -> bool {
        let n = match self {
            NodeChildRange::Exactly(n) => *n,
            NodeChildRange::UpTo(n) => *n,
        };
        current < n
    }
}

impl Default for NodeChildRange {
    fn default() -> Self {
        NodeChildRange::Exactly(0)
    }
}

pub trait NodeSettings: Default {
    fn name() -> &'static str;
    fn input() -> &'static [(&'static str, IOType)];
    fn output() -> &'static [(&'static str, IOType)];
    fn child_range() -> NodeChildRange {
        NodeChildRange::default()
    }
    fn build(self) -> anyhow::Result<Extras>;
}

pub trait IOSlot<T> {
    fn into_slot(self, name: &str) -> IO;
}

impl<T: IOBuilder> IOSlot<T> for T {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl<T: FromFloatUnchecked> IOSlot<T> for BindRoute<T> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl<T: FromFloatUnchecked> IOSlot<T> for BindParameter<T> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<bool> for BindRoute<bool> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<bool> for BindParameter<bool> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<[f32; 3]> for BindRoute<[f32; 3]> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<[f32; 3]> for BindParameter<[f32; 3]> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<Event> for BindRoute<Event> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<Event> for BindParameter<Event> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<Timer> for BindRoute<Timer> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<Timer> for BindParameter<Timer> {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<bool> for Query {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<bool> for Expression {
    fn into_slot(self, name: &str) -> IO {
        self.into_io(name)
    }
}

impl IOSlot<Event> for &str {
    fn into_slot(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_event(self.to_owned()))
    }
}

impl<'a> IOSlot<Event> for &'a String {
    fn into_slot(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_event(self.to_owned()))
    }
}

impl IOSlot<Event> for String {
    fn into_slot(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_event(self.to_owned()))
    }
}

impl<T: ResourceSettings> IOSlot<Resource<T>> for &str {
    fn into_slot(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_resource(self.to_owned()))
    }
}

impl<'a, T: ResourceSettings> IOSlot<Resource<T>> for &'a String {
    fn into_slot(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_resource(self.to_owned()))
    }
}

impl<T: ResourceSettings> IOSlot<Resource<T>> for String {
    fn into_slot(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_resource(self.to_owned()))
    }
}

pub trait IOBuilder {
    fn into_io(self, name: &str) -> IO;
}

impl<T: FromFloatUnchecked> IOBuilder for T {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_f32(self.into_f32()))
    }
}

impl IOBuilder for bool {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_bool(self))
    }
}

impl IOBuilder for Value {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_value(self))
    }
}

impl IOBuilder for [f32; 3] {
    fn into_io(self, name: &str) -> IO {
        IO::new(
            name,
            IOSettings::from_value(serde_json::to_value(self).expect("Valid")),
        )
    }
}

impl IOBuilder for SampleTimer {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_timer(self))
    }
}

impl<T: IOBuilder> IOBuilder for BindRoute<T> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_route(self.0))
    }
}

impl IOBuilder for BindRoute<Timer> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_route(self.0))
    }
}

impl IOBuilder for BindRoute<VectorRef> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_route(self.0))
    }
}

impl IOBuilder for BindRoute<Event> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_route(self.0))
    }
}

impl<T: IOBuilder> IOBuilder for BindParameter<T> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_parameter(self.0))
    }
}

impl IOBuilder for BindParameter<Event> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_parameter(self.0))
    }
}

impl IOBuilder for BindParameter<VectorRef> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_parameter(self.0))
    }
}

impl IOBuilder for BindParameter<Timer> {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_parameter(self.0))
    }
}

impl IOBuilder for Expression {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_expression(self))
    }
}

impl IOBuilder for Query {
    fn into_io(self, name: &str) -> IO {
        IO::new(name, IOSettings::from_expression(self.into()))
    }
}

impl<T: FromFloatUnchecked> From<T> for Expression {
    fn from(value: T) -> Self {
        let x: f32 = value.into_f32();
        if let Some(value) = Number::from_f64(x as f64).map(Value::Number) {
            Self::Value(value)
        } else {
            Self::None
        }
    }
}

impl From<bool> for Expression {
    fn from(value: bool) -> Self {
        Self::Value(Value::Bool(value))
    }
}

impl From<Query> for Expression {
    fn from(value: Query) -> Self {
        Self::Query(value)
    }
}

impl<T: FromFloatUnchecked> From<BindRoute<T>> for Expression {
    fn from(value: BindRoute<T>) -> Self {
        Self::Route(IORouteSettings {
            route: value.0,
            ..Default::default()
        })
    }
}

impl From<BindRoute<bool>> for Expression {
    fn from(value: BindRoute<bool>) -> Self {
        Self::Route(IORouteSettings {
            route: value.0,
            ..Default::default()
        })
    }
}

impl<T: FromFloatUnchecked> From<BindParameter<T>> for Expression {
    fn from(value: BindParameter<T>) -> Self {
        Self::Parameter(value.0.into())
    }
}

impl From<BindParameter<bool>> for Expression {
    fn from(value: BindParameter<bool>) -> Self {
        Self::Parameter(value.0.into())
    }
}

pub fn alias(alias: &str, node: Node) -> Node {
    node.with_alias(alias)
}

pub fn endpoint(node: Node) -> Branch {
    Branch {
        node: Some(node),
        ..Default::default()
    }
}

/// `node` is updated before branch
pub fn preprocess(node: Node, branch: impl Into<Branch>) -> Branch {
    Branch {
        node: Some(node),
        target: Some(BranchTarget::Postprocess(Box::new(branch.into()))),
        ..Default::default()
    }
}

pub fn submachine(name: &str) -> Branch {
    Branch {
        target: Some(BranchTarget::StateMachine(name.into())),
        ..Default::default()
    }
}

pub fn state(name: &str) -> State {
    State {
        name: name.into(),
        ..Default::default()
    }
}

pub fn state_machine(name: &str, states: impl Into<Vec<State>>) -> StateMachine {
    StateMachine {
        name: name.into(),
        states: states.into(),
        ..Default::default()
    }
}

pub fn graph_iteration() -> Expression {
    Expression::CompilerGlobal("graph_iteration".to_owned())
}

pub fn event_is(name: &str, query: QueryType) -> Query {
    Query::Event(EventQuery {
        event: name.into(),
        query,
    })
}

pub fn state_is(name: &str, query: QueryType) -> Query {
    Query::State(StateQuery {
        state: name.into(),
        query,
    })
}

pub fn state_machine_is(name: &str, query: QueryType) -> Query {
    Query::StateMachine(StateMachineQuery {
        state_machine: name.into(),
        query,
    })
}
