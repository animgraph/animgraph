use serde_derive::{Deserialize, Serialize};

use crate::Id;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoneGroupId {
    #[default]
    All,
    Reference(u16),
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BoneWeight {
    pub bone: Id,
    pub weight: f32,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BoneGroup {
    pub group: BoneGroupId,
    pub weights: Vec<BoneWeight>,
}

impl BoneGroup {
    pub const RESOURCE_TYPE: &str = "bone_group";

    #[cfg(feature = "compiler")]
    pub const IO_TYPE: crate::model::IOType = crate::model::IOType::Resource(Self::RESOURCE_TYPE);

    pub fn new<T: AsRef<str>>(id: u16, bones: impl Iterator<Item = T>) -> Self {
        Self {
            group: BoneGroupId::Reference(id),
            weights: bones
                .map(|x| BoneWeight {
                    bone: Id::from_str(x.as_ref()),
                    weight: 1.0,
                })
                .collect(),
        }
    }
}

impl super::ResourceSettings for BoneGroup {
    fn resource_type() -> &'static str {
        Self::RESOURCE_TYPE
    }
}
