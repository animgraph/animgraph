use std::error::Error;

use crate::{
    core::{Alpha, ALPHA_ONE, ALPHA_ZERO},
    interpreter::InterpreterContext,
    state_machine::NodeIndex,
};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerType {
    #[default]
    Endpoint,
    List,
    StateMachine,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LayerWeight(pub Alpha);

impl LayerWeight {
    pub const ZERO: Self = Self(ALPHA_ZERO);
    pub const ONE: Self = Self(ALPHA_ONE);

    #[inline]
    pub fn is_nearly_zero(&self) -> bool {
        self.0.is_nearly_zero()
    }

    #[inline]
    pub fn is_nearly_one(&self) -> bool {
        self.0.is_nearly_one()
    }

    #[inline]
    pub fn interpolate(a: Self, b: Self, t: Alpha) -> Self {
        Self(t.interpolate(a.0, b.0))
    }
}

#[derive(Debug, Default, Clone)]
pub struct Layer {
    pub context: InterpreterContext,
    pub layer_type: LayerType,
    pub node: Option<NodeIndex>,
    pub first_child: Option<usize>,
    pub last_child: Option<usize>,
    pub next_sibling: Option<usize>,
}

impl Layer {
    pub fn layer_weight(&self) -> Alpha {
        self.context.layer_weight.0
    }

    pub fn transition_weight(&self) -> Alpha {
        self.context.transition_weight
    }
}

#[derive(Default, Clone)]
pub struct LayerBuilder {
    pub layers: Vec<Layer>,
    pub layer_pointer: usize,
}

impl LayerBuilder {
    pub fn clear(&mut self) {
        self.layers.clear();
        self.layer_pointer = 0;
    }

    pub fn push_layer(
        &mut self,
        context: &InterpreterContext,
        layer_type: LayerType,
        node: Option<NodeIndex>,
    ) -> usize {
        let layer = Layer {
            context: context.clone(),
            layer_type,
            node,
            ..Layer::default()
        };
        let index = self.layers.len();

        if self.layers.is_empty() {
            self.layers.push(layer);
            self.layer_pointer = 0;
            return 0;
        }

        self.layers.push(layer);

        let parent = &mut self.layers[self.layer_pointer];
        if let Some(sibling) = parent.last_child.take() {
            parent.last_child = Some(index);
            let sibling = &mut self.layers[sibling];
            sibling.next_sibling = Some(index);
        } else {
            parent.first_child = Some(index);
            parent.last_child = Some(index);
        }

        match layer_type {
            LayerType::Endpoint => self.layer_pointer,
            LayerType::List | LayerType::StateMachine => {
                let parent = self.layer_pointer;
                self.layer_pointer = index;
                parent
            }
        }
    }

    pub fn apply_layer_weight(&mut self, alpha: Alpha) {
        let layer = &mut self.layers[self.layer_pointer];
        layer.context.layer_weight.0 *= alpha;
    }

    pub fn reset_layer_children(&mut self, context: &InterpreterContext) {
        let layer = &mut self.layers[self.layer_pointer];
        assert_eq!(context.machine, layer.context.machine);
        layer.first_child = None;
        layer.last_child = None;
    }

    pub fn pop_layer(&mut self, context: &InterpreterContext, parent: usize) {
        let layer = &mut self.layers[self.layer_pointer];

        assert_eq!(context.machine, layer.context.machine);
        layer.context.layer_weight = context.layer_weight;

        self.layer_pointer = parent;
    }

    pub fn blend_layer<T: BlendContext>(
        &self,
        context: &mut T,
        layer: &Layer,
    ) -> anyhow::Result<Option<T::Task>> {
        let task = match layer.layer_type {
            LayerType::Endpoint => {
                if let Some(node) = layer.node {
                    return context.sample(node);
                } else {
                    return Ok(None);
                }
            }
            LayerType::List => {
                let mut previous = 0;
                let mut next = layer.first_child;
                let mut source = None;
                while let Some(index) = next {
                    assert!(previous < index);

                    let Some(child) = self.layers.get(index) else {
                        return Err(InvalidLayerError.into());
                    };

                    if child.layer_weight().is_nearly_zero() {
                        previous = index;
                        next = child.next_sibling;
                        continue;
                    }

                    if let Some(b) = self.blend_layer(context, child)? {
                        source = if let Some(a) = source {
                            Some(context.blend_layers(a, b, child.layer_weight())?)
                        } else {
                            Some(b)
                        };
                    }

                    previous = index;
                    next = child.next_sibling;
                }

                source
            }

            LayerType::StateMachine => {
                let mut previous = 0;
                let mut next = layer.first_child;
                let mut source = None;
                while let Some(index) = next {
                    assert!(previous < index);

                    let Some(child) = self.layers.get(index) else {
                        return Err(InvalidLayerError.into());
                    };

                    if child.transition_weight().is_nearly_zero() && source.is_some() {
                        previous = index;
                        next = child.next_sibling;
                        continue;
                    }

                    if let Some(b) = self.blend_layer(context, child)? {
                        source = if let Some(a) = source {
                            Some(context.interpolate(a, b, child.transition_weight())?)
                        } else {
                            Some(b)
                        };
                    }

                    previous = index;
                    next = child.next_sibling;
                }

                source
            }
        };

        if let Some(child_task) = task {
            if let Some(parent) = layer.node {
                return Ok(Some(context.apply_parent(parent, child_task)?));
            }
            Ok(Some(child_task))
        } else {
            Ok(None)
        }
    }

    pub fn blend<T: BlendContext>(&self, context: &mut T) -> anyhow::Result<Option<T::Task>> {
        if let Some(layer) = self.layers.first() {
            self.blend_layer(context, layer)
        } else {
            Ok(None)
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct InvalidLayerError;

impl Error for InvalidLayerError {}

impl std::fmt::Display for InvalidLayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid layer")
    }
}
pub trait BlendContext {
    type Task;
    fn sample(&mut self, node: NodeIndex) -> anyhow::Result<Option<Self::Task>>;
    fn apply_parent(
        &mut self,
        parent: NodeIndex,
        child_task: Self::Task,
    ) -> anyhow::Result<Self::Task>;

    fn blend_layers(
        &mut self,
        a: Self::Task,
        b: Self::Task,
        w: Alpha,
    ) -> anyhow::Result<Self::Task>;
    fn interpolate(&mut self, a: Self::Task, b: Self::Task, w: Alpha)
        -> anyhow::Result<Self::Task>;
}
