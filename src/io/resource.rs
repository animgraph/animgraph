use std::marker::PhantomData;

use serde_derive::{Deserialize, Serialize};

use crate::{graph::Graph, state_machine::VariableIndex, IndexType};

#[derive(Debug, Serialize, Deserialize)]
pub struct Resource<T: 'static> {
    pub variable: IndexType,
    #[serde(skip)]
    _phantom: PhantomData<T>,
}

impl<T: 'static> Eq for Resource<T> {}
impl<T: 'static> PartialEq for Resource<T> {
    fn eq(&self, other: &Self) -> bool {
        self.variable == other.variable
    }
}

impl<T: 'static> Copy for Resource<T> {}

impl<T: 'static> Clone for Resource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Resource<T> {
    pub const fn new(variable: VariableIndex) -> Self {
        Self {
            variable: variable.0,
            _phantom: PhantomData,
        }
    }

    pub fn get<'a>(&self, graph: &'a Graph) -> Option<&'a T> {
        graph.get_resource(self.variable)
    }

    pub const INVALID: Self = Self::new(VariableIndex(IndexType::MAX));
}
