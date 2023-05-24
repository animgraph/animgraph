mod animation_clip;
mod bone_group;
mod skeleton;

#[cfg(feature = "compiler")]
pub mod model;

pub use animation_clip::*;
pub use bone_group::*;
use serde::Serialize;
pub use skeleton::*;

pub trait ResourceSettings : Serialize {
    fn resource_type() -> &'static str;

    #[cfg(feature = "compiler")]
    fn build_content(&self, name: &str) -> anyhow::Result<model::ResourceContent> {
        Ok(super::model::ResourceContent {
            name: name.to_owned(),
            content: serde_json::to_value(self)?,
            resource_type: Self::resource_type().to_owned(),
            id: uuid::Uuid::new_v4(),
        })
    }
}
