use serde_derive::{Deserialize, Serialize};

use crate::{
    core::{SampleTimer, Seconds},
    BoneGroupId,
};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct AnimationId(pub u32);

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnimationClip {
    pub animation: AnimationId,
    pub bone_group: BoneGroupId,
    pub looping: bool,
    pub start: Seconds,
    pub duration: Seconds,
}

impl AnimationClip {
    pub const RESOURCE_TYPE: &str = "animation_clip";

    pub fn init_timer(&self) -> SampleTimer {
        SampleTimer::new(self.start, self.duration, self.looping)
    }

    #[cfg(feature = "compiler")]
    pub const IO_TYPE: crate::model::IOType = crate::model::IOType::Resource(Self::RESOURCE_TYPE);
}

impl super::ResourceSettings for AnimationClip {
    fn resource_type() -> &'static str {
        Self::RESOURCE_TYPE
    }
}
