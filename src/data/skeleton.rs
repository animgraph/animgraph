use std::collections::{BTreeMap, VecDeque};

use serde_derive::{Deserialize, Serialize};

use crate::{Id, Transform};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct SkeletonId(pub u32);

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Skeleton {
    pub id: SkeletonId,
    pub bones: Vec<Id>,
    pub parents: Vec<Option<u16>>,
}

pub static EMPTY_SKELETON: Skeleton = Skeleton {
    id: SkeletonId(u32::MAX),
    bones: vec![],
    parents: vec![],
};

impl Skeleton {
    pub const RESOURCE_TYPE: &str = "skeleton";

    #[cfg(feature = "compiler")]
    pub const IO_TYPE: crate::model::IOType = crate::model::IOType::Resource(Self::RESOURCE_TYPE);

    pub fn transform_by_id<'a>(&self, pose: &'a [Transform], id: Id) -> Option<&'a Transform> {
        self.bones
            .iter()
            .position(|x| *x == id)
            .and_then(|index| pose.get(index))
    }

    /// Produces a skeleton with topologicaly sorted bones from bones with empty parent
    /// or parents not in the list, and ignores any cyclic clusters
    pub fn from_parent_map<T: AsRef<str> + Ord>(bones: &BTreeMap<T, T>) -> Self {
        let mut queue: VecDeque<&T> = VecDeque::new();

        for (bone, parent) in bones.iter() {
            if parent.as_ref().is_empty() {
                if !queue.contains(&bone) {
                    queue.push_back(bone);
                }
            } else if !bones.contains_key(parent) && !queue.contains(&parent) {
                queue.push_back(parent);
            }
        }

        let mut result: Vec<&T> = Vec::new();
        let mut parents = Vec::new();

        while let Some(bone) = queue.pop_front() {
            if result.contains(&bone) {
                continue;
            }
            if let Some(parent) = bones.get(bone).filter(|x| !x.as_ref().is_empty()) {
                let index = result
                    .iter()
                    .position(|&x| x == parent)
                    .expect("Parents should be placed in result before children");
                parents.push(Some(index as u16));
            } else {
                parents.push(None);
            }
            result.push(bone);

            for (child, parent) in bones.iter() {
                if parent == bone {
                    queue.push_front(child);
                }
            }
        }

        Self {
            id: SkeletonId(0),
            bones: result.iter().map(|&x| Id::from_str(x.as_ref())).collect(),
            parents,
        }
    }
}

impl super::ResourceSettings for Skeleton {
    fn resource_type() -> &'static str {
        Self::RESOURCE_TYPE
    }
}

#[cfg(test)]
#[test]
fn test_parent_map() {
    let bones: BTreeMap<_, _> = [
        ("Head", "Neck"),
        ("LeftArm", "LeftShoulder"),
        ("LeftFoot", "LeftLeg"),
        ("LeftForeArm", "LeftArm"),
        ("LeftHand", "LeftForeArm"),
        ("LeftHandIndex1", "LeftHand"),
        ("LeftHandIndex2", "LeftHandIndex1"),
        ("LeftHandIndex3", "LeftHandIndex2"),
        ("LeftHandMiddle1", "LeftHand"),
        ("LeftHandMiddle2", "LeftHandMiddle1"),
        ("LeftHandMiddle3", "LeftHandMiddle2"),
        ("LeftHandPinky1", "LeftHand"),
        ("LeftHandPinky2", "LeftHandPinky1"),
        ("LeftHandPinky3", "LeftHandPinky2"),
        ("LeftHandRing1", "LeftHand"),
        ("LeftHandRing2", "LeftHandRing1"),
        ("LeftHandRing3", "LeftHandRing2"),
        ("LeftHandThumb1", "LeftHand"),
        ("LeftHandThumb2", "LeftHandThumb1"),
        ("LeftHandThumb3", "LeftHandThumb2"),
        ("LeftLeg", "LeftUpLeg"),
        ("LeftShoulder", "Spine2"),
        ("LeftToeBase", "LeftFoot"),
        ("LeftUpLeg", "Hips"),
        ("Neck", "Spine2"),
        ("RightArm", "RightShoulder"),
        ("RightFoot", "RightLeg"),
        ("RightForeArm", "RightArm"),
        ("RightHand", "RightForeArm"),
        ("RightHandIndex1", "RightHand"),
        ("RightHandIndex2", "RightHandIndex1"),
        ("RightHandIndex3", "RightHandIndex2"),
        ("RightHandMiddle1", "RightHand"),
        ("RightHandMiddle2", "RightHandMiddle1"),
        ("RightHandMiddle3", "RightHandMiddle2"),
        ("RightHandPinky1", "RightHand"),
        ("RightHandPinky2", "RightHandPinky1"),
        ("RightHandPinky3", "RightHandPinky2"),
        ("RightHandRing1", "RightHand"),
        ("RightHandRing2", "RightHandRing1"),
        ("RightHandRing3", "RightHandRing2"),
        ("RightHandThumb1", "RightHand"),
        ("RightHandThumb2", "RightHandThumb1"),
        ("RightHandThumb3", "RightHandThumb2"),
        ("RightLeg", "RightUpLeg"),
        ("RightShoulder", "Spine2"),
        ("RightToeBase", "RightFoot"),
        ("RightUpLeg", "Hips"),
        ("Spine", "Hips"),
        ("Spine1", "Spine"),
        ("Spine2", "Spine1"),
    ]
    .into_iter()
    .collect();

    let skeleton = Skeleton::from_parent_map(&bones);
    assert_eq!(skeleton.bones.len(), 52);
    assert_eq!(skeleton.bones[0], Id::from_str("Hips"));
}

#[cfg(test)]
#[test]
fn test_recursive_parent_map() {
    let bones: BTreeMap<_, _> = [("Spine2", "Spine1"), ("Spine1", "Spine2")]
        .into_iter()
        .collect();

    let skeleton = Skeleton::from_parent_map(&bones);
    assert!(skeleton.bones.is_empty());
}
