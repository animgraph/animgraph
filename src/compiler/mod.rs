use std::collections::{BTreeMap, HashMap};

use serde_json::{Number, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    compiler::context::IndexedPath,
    io::{BoolMut, Event, NumberMut},
    model::{AnimGraph, Node, DEFAULT_OUPTUT_NAME},
    state_machine::{ConstantIndex, NodeIndex},
    GraphBoolean, GraphDebugBreak, GraphDefinitionBuilder, GraphNumber, IndexType,
};

use self::{
    context::{run_graph_definition_compilation, CompileError, GraphCompilationContext},
    prelude::{NodeCompilationOutput, NodeCompilationRegistry},
};

pub mod context;

#[cfg(test)]
mod tests;

pub mod prelude {
    pub use super::context::*;
    pub use crate::io::*;
    pub use crate::processors::compile::*;
    pub use crate::processors::*;
    pub use crate::state_machine::*;
    pub use crate::*;

    pub use crate::data::model::*;

    pub use super::GraphDefinitionCompilation;
}

#[derive(Default, Clone)]
pub struct GraphDefinitionCompilation {
    pub builder: GraphDefinitionBuilder,
    pub output_lookup: HashMap<NodeCompilationOutput, String>,
    pub events: Vec<String>,
    pub events_lookup: HashMap<String, Event>,
    pub parameters_lookup: HashMap<String, NodeCompilationOutput>,
    pub resources_lookup: HashMap<String, NodeCompilationOutput>,
    pub numbers_lookup: HashMap<Number, ConstantIndex>,
    pub global_booleans: HashMap<String, GraphBoolean>,
    pub global_number: HashMap<String, GraphNumber>,
    pub debug_triggers: BTreeMap<IndexType, Value>,
    pub node_aliases: Vec<String>,
}

impl GraphDefinitionCompilation {
    pub fn compile(
        graph: &AnimGraph,
        registry: &NodeCompilationRegistry,
    ) -> Result<Box<GraphDefinitionCompilation>, CompileError> {
        let mut definition = Box::<GraphDefinitionCompilation>::default();
        definition
            .global_number
            .insert("graph_iteration".to_owned(), GraphNumber::Iteration);

        let mut context = GraphCompilationContext::build_context(graph, registry)?;
        run_graph_definition_compilation(&mut definition, &mut context)?;

        Ok(definition)
    }
}

impl GraphDefinitionCompilation {
    pub fn get_debug_trigger(&self, trigger: &GraphDebugBreak) -> Option<&Value> {
        match trigger {
            GraphDebugBreak::Condition { condition_index } => {
                self.debug_triggers.get(condition_index)
            }
        }
    }

    pub fn get_boolean_parameter(&self, name: &str) -> Option<BoolMut> {
        self.parameters_lookup.get(name).and_then(|x| x.as_bool())
    }

    pub fn get_number_parameter(&self, name: &str) -> Option<NumberMut<f32>> {
        self.parameters_lookup.get(name).and_then(|x| x.as_number())
    }

    pub fn event_by_name(&self, name: &str) -> Option<Event> {
        self.events_lookup.get(name).cloned()
    }

    pub fn node_alias(&self, index: NodeIndex) -> Option<&str> {
        self.node_aliases.get(index.0 as usize).map(|x| x.as_str())
    }
}

pub struct NodeRoute<'a> {
    pub path: IndexedPath<'a>,
    pub node: &'a crate::model::Node,
}

pub fn compute_node_routes(graph: &AnimGraph) -> Vec<NodeRoute<'_>> {
    use crate::model::*;

    fn visit_node<'a>(
        node: &'a Node,
        paths: &mut context::IndexedPath<'a>,
        ids: &mut Vec<NodeRoute<'a>>,
    ) {
        ids.push(NodeRoute {
            path: paths.clone(),
            node,
        });

        for (index, child) in node.children.iter().enumerate() {
            paths.push(Some(index), &child.name);
            visit_node(child, paths, ids);
            paths.pop(Some(index), &child.name);
        }
    }

    fn visit_root<'a>(
        node: &'a Node,
        paths: &mut context::IndexedPath<'a>,
        ids: &mut Vec<NodeRoute<'a>>,
    ) {
        paths.push(Some(0), context::TREE_ROOT_SCOPE);

        visit_node(node, paths, ids);
        paths.pop(Some(0), context::TREE_ROOT_SCOPE);
    }

    fn visit_branch<'a>(
        branch: &'a Branch,
        paths: &mut context::IndexedPath<'a>,
        ids: &mut Vec<NodeRoute<'a>>,
    ) {
        if let Some(node) = branch.node.as_ref() {
            visit_root(node, paths, ids);
        }

        if let Some(target) = branch.target.as_ref() {
            match target {
                BranchTarget::Postprocess(branch) => {
                    paths.add_scope(Some(0), "preprocess");
                    visit_branch(branch, paths, ids);
                    paths.remove_scope(Some(0), "preprocess");
                }
                BranchTarget::Layers(layers) => {
                    for (index, branch) in layers.iter().enumerate() {
                        paths.add_scope(Some(index), "");
                        visit_branch(branch, paths, ids);
                        paths.remove_scope(Some(index), "");
                    }
                }
                BranchTarget::StateMachine { .. } => {}
            }
        }
    }

    let mut result = Vec::new();
    let mut paths = IndexedPath::default();

    for (machine_index, machine) in graph.state_machines.iter().enumerate() {
        paths.add_scope(Some(machine_index), &machine.name);
        for (state_index, state) in machine.states.iter().enumerate() {
            paths.add_scope(Some(state_index), &state.name);

            paths.add_scope(Some(0), context::BRANCH_SCOPE);
            visit_branch(&state.branch, &mut paths, &mut result);
            paths.remove_scope(Some(0), context::BRANCH_SCOPE);

            paths.add_scope(Some(1), context::TRANSITION_SCOPE);
            for (transition_index, transition) in state.transitions.iter().enumerate() {
                paths.add_scope(Some(transition_index), &transition.target.name);
                if let Some(node) = transition.node.as_ref() {
                    visit_root(node, &mut paths, &mut result);
                }
                paths.remove_scope(Some(transition_index), &transition.target.name);
            }
            paths.remove_scope(Some(1), context::TRANSITION_SCOPE);

            paths.remove_scope(Some(state_index), &state.name);
        }
        paths.remove_scope(Some(machine_index), &machine.name);
    }

    result
}

#[derive(Error, Debug)]
pub enum NodeRouteError {
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

pub struct NodeRouteContext {
    pub aliases: HashMap<String, String>,
}

fn resolve_root_path<'a>(
    route: &IndexedPath,
    path: &[(Option<usize>, &str)],
    graph: &'a AnimGraph,
) -> Result<&'a crate::model::Node, NodeRouteError> {
    if path.len() < 3 {
        return Err(NodeRouteError::InvalidRoute(route.join()));
    }

    let (machine_index, machine_name) = path[0];
    let state_machine =
        if let Some(sm) = graph.state_machines.iter().find(|x| x.name == machine_name) {
            sm
        } else if let Some(sm) = machine_index.and_then(|x| graph.state_machines.get(x)) {
            sm
        } else {
            return Err(NodeRouteError::UnknownMachine(route.join()));
        };

    let (state_index, state_name) = path[1];
    let state = if let Some(state) = state_machine.states.iter().find(|x| x.name == state_name) {
        state
    } else if let Some(state) = state_index.and_then(|x| state_machine.states.get(x)) {
        state
    } else {
        return Err(NodeRouteError::UnknownState(route.join()));
    };

    let (scope_index, state_scope) = path[2];

    if scope_index == Some(0) || (scope_index.is_none() && state_scope == context::BRANCH_SCOPE) {
        if !state_scope.is_empty() && state_scope != context::BRANCH_SCOPE {
            return Err(NodeRouteError::InvalidRoute(route.join()));
        }

        let mut branch = &state.branch;
        for (index, _name) in path.iter().skip(3) {
            branch = branch
                .target
                .as_ref()
                .and_then(|x| match x {
                    crate::model::BranchTarget::Postprocess(branch)
                        if index.unwrap_or_default() == 0 =>
                    {
                        Some(branch.as_ref())
                    }
                    crate::model::BranchTarget::Layers(list) => index.and_then(|i| list.get(i)),
                    _ => None,
                })
                .ok_or_else(|| NodeRouteError::InvalidBranch(route.join()))?;
        }

        if let Some(node) = branch.node.as_ref() {
            Ok(node)
        } else {
            Err(NodeRouteError::InvalidRoute(route.join()))
        }
    } else if scope_index == Some(1)
        || (scope_index.is_none() && state_scope == context::TRANSITION_SCOPE)
    {
        if !state_scope.is_empty() && state_scope != context::TRANSITION_SCOPE {
            return Err(NodeRouteError::InvalidRoute(route.join()));
        }

        // ::<Machine>::<State>::transition::<State>/node
        if path.len() != 4 || state.transitions.is_empty() {
            return Err(NodeRouteError::InvalidTransition(route.join()));
        }

        let (transition_index, transition_scope) = path[3];

        let transition = transition_index
            .and_then(|x| state.transitions.get(x))
            .or_else(|| {
                state
                    .transitions
                    .iter()
                    .find(|x| x.target.name == transition_scope)
            })
            .ok_or_else(|| NodeRouteError::InvalidTransition(route.join()))?;

        if !transition_scope.is_empty() && transition.target.name != transition_scope {
            return Err(NodeRouteError::InvalidTransition(route.join()));
        }

        if let Some(node) = transition.node.as_ref() {
            return Ok(node);
        } else {
            return Err(NodeRouteError::InvalidRoute(route.join()));
        }
    } else {
        return Err(NodeRouteError::InvalidRoute(route.join()));
    }
}

fn resolve_node_relative<'a>(
    route: &str,
    mut node: &'a crate::model::Node,
    mut path: &[(Option<usize>, &str)],
) -> Result<&'a crate::model::Node, NodeRouteError> {
    while let Some(&(index, name)) = path.first() {
        path = &path[1..];

        node = 'found: {
            for (child_index, child) in node.children.iter().enumerate() {
                let child_name = &child.name;
                if index == Some(child_index) || (index.is_none() && name == child_name) {
                    if (index.is_some() && index != Some(child_index))
                        || (!name.is_empty() && name != child_name)
                    {
                        return Err(NodeRouteError::InvalidNode(route.to_owned()));
                    }

                    break 'found child;
                }
            }

            return Err(NodeRouteError::InvalidRoute(route.to_owned()));
        };
    }

    Ok(node)
}

fn resolve_route<'a>(
    context: &context::ContextPath,
    mut route: &'a str,
    graph: &'a AnimGraph,
    aliases: &HashMap<String, &'a Node>,
) -> Result<(&'a Node, &'a str), NodeRouteError> {
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
            if let Some(&node) = aliases.get(alias) {
                let indexed_path = IndexedPath::from_route(route);

                if indexed_path.path.len() < 2 {
                    return Err(NodeRouteError::InvalidRoute(route.to_owned()));
                }

                let path = &indexed_path.path[1..];
                return Ok((resolve_node_relative(route, node, path)?, output_slot));
            }
        } else if let Some(&node) = aliases.get(route) {
            return Ok((node, output_slot));
        }
    }
    let indexed_path = context.append(route);
    let node = resolve_root_path(&indexed_path, &indexed_path.scope, graph)?;
    let mut path = indexed_path.path.as_slice();
    match path.first() {
        Some(&(index, name))
            if (name.is_empty() || (context::TREE_ROOT_SCOPE == name))
                && index.unwrap_or_default() == 0 => {}
        _ => return Err(NodeRouteError::InvalidRoute(route.to_owned())),
    }
    path = &path[1..];

    Ok((resolve_node_relative(route, node, path)?, output_slot))
}

fn visit_expr_routes_mut(
    expr: &mut crate::model::Expression,
    visitor: &mut impl FnMut(&mut crate::model::IORouteSettings),
) {
    use crate::model::Expression::*;
    expr.visit_mut(&mut |e| {
        if let Route(route) = e {
            visitor(route)
        }
    });
}

pub fn resolve_routes(graph: &mut AnimGraph) -> Result<(), Vec<String>> {
    fn visit_expr_routes(expr: &crate::model::Expression, visitor: &mut impl FnMut(&String)) {
        use crate::model::Expression::*;

        expr.visit(&mut |e| {
            if let Route(route) = e {
                visitor(&route.route)
            }
        });
    }

    graph.generate_id_for_nils();

    let mut lookup = HashMap::new();
    let mut ordered_lookup = Vec::new();

    {
        let routes: Vec<NodeRoute> = compute_node_routes(graph);
        let mut aliases = HashMap::with_capacity(routes.len());
        for route in routes.iter() {
            aliases.insert(route.node.alias.clone(), route.node);
        }

        for node_route in routes.iter() {
            let context_path = context::ContextPath::ParentOf(&node_route.path);

            for input in node_route.node.input.iter() {
                match &input.value {
                    crate::model::IOSettings::Route(route) => {
                        let result = resolve_route(&context_path, &route.route, graph, &aliases)
                            .map(|(node, path)| (node.id, path.to_owned()));

                        if result.is_err() {
                            dbg!(&result);
                        }
                        lookup.insert((node_route.node.id, route.route.clone()), result);
                    }
                    crate::model::IOSettings::Expression(expr) => {
                        let expr = &expr.expression;

                        visit_expr_routes(expr, &mut |route: &String| {
                            let result = resolve_route(&context_path, route, graph, &aliases)
                                .map(|(node, path)| (node.id, path.to_owned()));

                            lookup.insert((node_route.node.id, route.clone()), result);
                        });
                    }

                    _ => {}
                }
            }
        }

        let mut paths = IndexedPath::default();
        for (machine_index, machine) in graph.state_machines.iter().enumerate() {
            paths.add_scope(Some(machine_index), &machine.name);
            for (state_index, state) in machine.states.iter().enumerate() {
                paths.add_scope(Some(state_index), &state.name);

                // global condition
                let context_path = context::ContextPath::Path(&paths);
                visit_expr_routes(&state.global_condition, &mut |route| {
                    let result = resolve_route(&context_path, route, graph, &aliases)
                        .map(|(node, path)| (node.id, path.to_owned()));
                    ordered_lookup.push(result);
                });

                // skip branch

                paths.add_scope(Some(1), context::TRANSITION_SCOPE);
                for (transition_index, transition) in state.transitions.iter().enumerate() {
                    paths.add_scope(Some(transition_index), &transition.target.name);

                    // transition conditions and values
                    let context_path = context::ContextPath::Path(&paths);
                    visit_expr_routes(&transition.condition, &mut |route| {
                        let result = resolve_route(&context_path, route, graph, &aliases)
                            .map(|(node, path)| (node.id, path.to_owned()));
                        ordered_lookup.push(result);
                    });

                    for io in [
                        &transition.progress,
                        &transition.blend,
                        &transition.duration,
                    ] {
                        match &io.value {
                            crate::model::IOSettings::Route(route) => {
                                let result =
                                    resolve_route(&context_path, &route.route, graph, &aliases)
                                        .map(|(node, path)| (node.id, path.to_owned()));
                                ordered_lookup.push(result);
                            }
                            crate::model::IOSettings::Expression(expr) => {
                                let expr = &expr.expression;

                                visit_expr_routes(expr, &mut |route: &String| {
                                    let result =
                                        resolve_route(&context_path, route, graph, &aliases)
                                            .map(|(node, path)| (node.id, path.to_owned()));

                                    ordered_lookup.push(result);
                                });
                            }

                            _ => {}
                        }
                    }

                    paths.remove_scope(Some(transition_index), &transition.target.name);
                }
                paths.remove_scope(Some(1), context::TRANSITION_SCOPE);

                paths.remove_scope(Some(state_index), &state.name);
            }
            paths.remove_scope(Some(machine_index), &machine.name);
        }
    }

    type Lookup = HashMap<(Uuid, String), Result<(Uuid, String), NodeRouteError>>;

    fn apply_route_ids(graph: &mut AnimGraph, lookup: &Lookup) {
        graph.visit_nodes_mut(&mut |node| {
            let node_id = node.id;
            for input in node.input.iter_mut() {
                match &mut input.value {
                    crate::model::IOSettings::Route(route) => {
                        if let Some(Ok((target_id, output))) =
                            lookup.get(&(node_id, route.route.clone()))
                        {
                            route.target.uuid = *target_id;
                            route.target.name = output.clone();
                        }
                    }
                    crate::model::IOSettings::Expression(expr) => {
                        let expr = &mut expr.expression;

                        visit_expr_routes_mut(expr, &mut |route| {
                            if let Some(Ok((target_id, output))) =
                                lookup.get(&(node_id, route.route.clone()))
                            {
                                route.target.uuid = *target_id;
                                route.target.name = output.clone();
                            }
                        });
                    }

                    _ => {}
                }
            }
        });
    }

    apply_route_ids(graph, &lookup);
    let mut ordered = ordered_lookup.iter();

    for machine in graph.state_machines.iter_mut() {
        for state in machine.states.iter_mut() {
            // global condition
            visit_expr_routes_mut(&mut state.global_condition, &mut |route| {
                if let Some(Ok((target_id, output))) = ordered.next() {
                    route.target.uuid = *target_id;
                    route.target.name = output.clone();
                }
            });

            // skip branch

            for transition in state.transitions.iter_mut() {
                visit_expr_routes_mut(&mut transition.condition, &mut |route| {
                    if let Some(Ok((target_id, output))) = ordered.next() {
                        route.target.uuid = *target_id;
                        route.target.name = output.clone();
                    }
                });

                for io in [
                    &mut transition.progress,
                    &mut transition.blend,
                    &mut transition.duration,
                ] {
                    match &mut io.value {
                        crate::model::IOSettings::Route(route) => {
                            if let Some(Ok((target_id, output))) = ordered.next() {
                                route.target.uuid = *target_id;
                                route.target.name = output.clone();
                            }
                        }
                        crate::model::IOSettings::Expression(expr) => {
                            let expr = &mut expr.expression;

                            visit_expr_routes_mut(expr, &mut |route| {
                                if let Some(Ok((target_id, output))) = ordered.next() {
                                    route.target.uuid = *target_id;
                                    route.target.name = output.clone();
                                }
                            });
                        }

                        _ => {}
                    }
                }
            }
        }
    }

    let mut route_errors: Vec<String> = ordered_lookup
        .into_iter()
        .filter_map(|x| x.err().map(|e| format!("{e}")))
        .collect();

    route_errors.extend(lookup.into_iter().filter_map(|((node, route), y)| {
        y.err()
            .map(|e| format!("node_id: {node:?}, route: {route}, {e}"))
    }));

    if route_errors.is_empty() {
        Ok(())
    } else {
        Err(route_errors)
    }
}

pub fn serialize_routes(graph: &mut AnimGraph) {
    let routes = compute_node_routes(graph);
    let mut lookup = HashMap::with_capacity(routes.len());

    for route in routes {
        lookup.insert(route.node.id, route.path.join());
    }

    use crate::model::{IORouteSettings, IOSettings};
    let mut visit_route = |route: &mut IORouteSettings| {
        if let Some(target) = lookup.get(&route.target.uuid) {
            assert!(!target.ends_with('/'));
            if route.target.name.is_empty() {
                route.route = target.clone();
            } else {
                route.route = format!("{target}.{}", route.target.name);
            }
        }
    };

    graph.visit_nodes_mut(&mut |node| {
        for io in node.input.iter_mut() {
            match &mut io.value {
                IOSettings::Route(route) => {
                    visit_route(route);
                }
                IOSettings::Expression(expr) => {
                    let expr = &mut expr.expression;

                    visit_expr_routes_mut(expr, &mut visit_route);
                }

                _ => {}
            }
        }
    });

    for machine in graph.state_machines.iter_mut() {
        for state in machine.states.iter_mut() {
            // global condition
            visit_expr_routes_mut(&mut state.global_condition, &mut visit_route);

            for transition in state.transitions.iter_mut() {
                visit_expr_routes_mut(&mut transition.condition, &mut visit_route);

                for io in [
                    &mut transition.progress,
                    &mut transition.blend,
                    &mut transition.duration,
                ] {
                    match &mut io.value {
                        IOSettings::Route(route) => {
                            visit_route(route);
                        }
                        IOSettings::Expression(expr) => {
                            let expr = &mut expr.expression;
                            visit_expr_routes_mut(expr, &mut visit_route);
                        }

                        _ => {}
                    }
                }
            }
        }
    }
}
