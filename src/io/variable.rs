use std::{fmt::Debug, marker::PhantomData};

use serde_derive::{Deserialize, Serialize};

use crate::{
    graph::{Graph, GraphNumber},
    state_machine::VariableIndex,
    FromFloatUnchecked, GraphBoolean, IndexType,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct NumberRef<T: FromFloatUnchecked + 'static> {
    index: GraphNumber,
    #[serde(skip)]
    _phantom: PhantomData<T>,
}

impl<T: FromFloatUnchecked + 'static> core::hash::Hash for NumberRef<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<T: FromFloatUnchecked + 'static> Eq for NumberRef<T> {}
impl<T: FromFloatUnchecked + 'static> PartialEq for NumberRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T: FromFloatUnchecked + 'static> Copy for NumberRef<T> {}

impl<T: FromFloatUnchecked + 'static> Clone for NumberRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: FromFloatUnchecked + 'static> Default for NumberRef<T> {
    fn default() -> Self {
        Self {
            index: GraphNumber::Zero,
            _phantom: Default::default(),
        }
    }
}

impl<T: FromFloatUnchecked + 'static> NumberRef<T> {
    pub fn new(index: GraphNumber) -> Self {
        NumberRef {
            index,
            _phantom: PhantomData,
        }
    }

    pub fn variable(&self) -> GraphNumber {
        self.index
    }

    pub fn get(&self, graph: &Graph) -> T {
        FromFloatUnchecked::from_f64(graph.get_number(self.index))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NumberMut<T: FromFloatUnchecked> {
    pub index: VariableIndex,
    #[serde(skip)]
    _phantom: PhantomData<T>,
}

impl<T: FromFloatUnchecked + 'static> core::hash::Hash for NumberMut<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<T: FromFloatUnchecked + 'static> Eq for NumberMut<T> {}
impl<T: FromFloatUnchecked + 'static> PartialEq for NumberMut<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T: FromFloatUnchecked + 'static> Copy for NumberMut<T> {}

impl<T: FromFloatUnchecked + 'static> Clone for NumberMut<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: FromFloatUnchecked> Default for NumberMut<T> {
    fn default() -> Self {
        Self {
            index: VariableIndex(IndexType::MAX),
            _phantom: Default::default(),
        }
    }
}

impl<T: FromFloatUnchecked> NumberMut<T> {
    pub fn new(variable: VariableIndex) -> Self {
        Self {
            index: variable,
            _phantom: PhantomData,
        }
    }

    pub fn variable(&self) -> GraphNumber {
        GraphNumber::Variable(self.index)
    }

    pub fn get(&self, graph: &Graph) -> T {
        FromFloatUnchecked::from_f32(graph.get_variable_number(self.index))
    }

    pub fn set(&self, graph: &mut Graph, value: T) {
        let input: f32 = value.into_f32();
        graph.set_variable_number(self.index, input);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoolMut {
    pub index: VariableIndex,
}

impl Default for BoolMut {
    fn default() -> Self {
        Self {
            index: VariableIndex(IndexType::MAX),
        }
    }
}

impl BoolMut {
    pub fn new(variable: VariableIndex) -> Self {
        Self { index: variable }
    }

    pub fn variable(&self) -> GraphBoolean {
        GraphBoolean::Variable(self.index)
    }

    pub fn get(&self, graph: &Graph) -> bool {
        graph.get_variable_boolean(self.index)
    }

    pub fn set(&self, graph: &mut Graph, value: bool) {
        graph.set_variable_boolean(self.index, value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VectorRef {
    Constant([f32; 3]),
    Variable(VariableIndex),
}

impl Default for VectorRef {
    fn default() -> Self {
        Self::Constant(Default::default())
    }
}

impl VectorRef {
    pub fn get(&self, graph: &Graph) -> [f32; 3] {
        match self {
            VectorRef::Constant(value) => *value,
            &VectorRef::Variable(index) => graph.get_variable_number_array(index),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VectorMut(pub VariableIndex);

impl Default for VectorMut {
    fn default() -> Self {
        Self(VariableIndex(IndexType::MAX))
    }
}

impl VectorMut {
    pub fn get(&self, graph: &Graph) -> [f32; 3] {
        graph.get_variable_number_array(self.0)
    }

    pub fn set(&self, graph: &mut Graph, value: [f32; 3]) {
        graph.set_variable_number_array(self.0, value);
    }
}
