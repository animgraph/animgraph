use crate::{
    processors::GraphNode, state_machine::NodeIndex, Alpha, AnimationId, BlendContext, BoneGroupId,
    Graph, LayerBuilder,
};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BlendSampleId {
    #[default]
    Reference,
    Task(u16),
}

#[derive(Debug, Clone)]
pub enum BlendSample {
    Animation {
        id: AnimationId,
        normalized_time: f32,
    },
    Blend(BlendSampleId, BlendSampleId, f32, BoneGroupId),
    Interpolate(BlendSampleId, BlendSampleId, f32),
}

impl PartialEq for BlendSample {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Animation {
                    id: l_id,
                    normalized_time: l_normalized_time,
                },
                Self::Animation {
                    id: r_id,
                    normalized_time: r_normalized_time,
                },
            ) => l_id == r_id && (l_normalized_time - r_normalized_time).abs() < 1e-6,
            (Self::Blend(l0, l1, l2, l3), Self::Blend(r0, r1, r2, r3)) => {
                l0 == r0 && l1 == r1 && (l2 - r2).abs() < 1e-6 && l3 == r3
            }
            (Self::Interpolate(l0, l1, l2), Self::Interpolate(r0, r1, r2)) => {
                l0 == r0 && l1 == r1 && (l2 - r2).abs() < 1e-6
            }
            _ => false,
        }
    }
}

#[derive(Default, Clone)]
pub struct BlendTree {
    tasks: Vec<BlendSample>,
    reference_task: Option<BlendSampleId>,

    blend_mask: BoneGroupId,
}

impl BlendTree {
    pub fn clear(&mut self) {
        self.tasks.clear();
        self.reference_task = None;
        self.blend_mask = BoneGroupId::All;
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn with_capacity(len: usize) -> Self {
        BlendTree {
            tasks: Vec::with_capacity(len),
            reference_task: None,
            blend_mask: BoneGroupId::All,
        }
    }

    pub fn from_vec(tasks: Vec<BlendSample>) -> Self {
        Self {
            tasks,
            reference_task: None,
            blend_mask: BoneGroupId::All,
        }
    }

    pub fn into_inner(self) -> Vec<BlendSample> {
        self.tasks
    }

    pub fn get(&self) -> &[BlendSample] {
        &self.tasks
    }

    fn push(&mut self, sample: BlendSample) -> BlendSampleId {
        let task = BlendSampleId::Task(self.tasks.len() as _);
        self.tasks.push(sample);
        task
    }

    pub fn get_reference_task(&mut self) -> Option<BlendSampleId> {
        self.reference_task
    }

    pub fn set_reference_task(&mut self, task: Option<BlendSampleId>) {
        self.reference_task = task;
    }

    pub fn sample_animation_clip(&mut self, animation: AnimationId, time: Alpha) -> BlendSampleId {
        self.push(BlendSample::Animation {
            id: animation,
            normalized_time: time.0,
        })
    }

    pub fn interpolate(&mut self, a: BlendSampleId, b: BlendSampleId, w: Alpha) -> BlendSampleId {
        if a == b {
            return a;
        }
        self.push(BlendSample::Interpolate(a, b, w.0))
    }

    pub fn blend_masked(&mut self, a: BlendSampleId, b: BlendSampleId, w: Alpha) -> BlendSampleId {
        if a == b {
            self.blend_mask = BoneGroupId::All;
            return a;
        }
        let id = self.push(BlendSample::Blend(a, b, w.0, self.blend_mask));
        self.blend_mask = BoneGroupId::All;
        id
    }

    pub fn append(
        &mut self,
        graph: &Graph,
        layers: &LayerBuilder,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        let mut context = PoseBlendContext(graph, self);
        let res = layers.blend(&mut context)?;
        if let Some(task) = res {
            self.reference_task = Some(task);
        }
        Ok(res)
    }

    pub fn set(
        &mut self,
        graph: &Graph,
        layers: &LayerBuilder,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        self.clear();
        self.append(graph, layers)
    }

    pub fn apply_mask(&mut self, sample: BlendSampleId, group: BoneGroupId) -> BlendSampleId {
        self.blend_mask = group;
        sample
    }

    pub fn visit<T: BlendTreeVisitor>(&self, visitor: &mut T, sample: BlendSampleId) {
        match sample {
            BlendSampleId::Reference => todo!(),
            BlendSampleId::Task(task) => {
                visitor.visit(self, self.tasks.get(task as usize).expect("Valid task"));
            }
        }
    }

    pub fn visit_root<T: BlendTreeVisitor>(&self, visitor: &mut T) {
        if let Some(root) = self.tasks.last() {
            visitor.visit(self, root);
        }
    }
}

pub struct PoseBlendContext<'a>(pub &'a Graph, pub &'a mut BlendTree);

impl<'a> BlendContext for PoseBlendContext<'a> {
    type Task = BlendSampleId;

    fn sample(&mut self, node: NodeIndex) -> anyhow::Result<Option<Self::Task>> {
        self.0.sample_pose(self.1, node)
    }

    fn apply_parent(
        &mut self,
        parent: NodeIndex,
        child_task: Self::Task,
    ) -> anyhow::Result<Self::Task> {
        if let Some(parent) = self.0.pose_parent(parent) {
            parent.apply_parent(self.1, child_task, self.0)
        } else {
            Ok(child_task)
        }
    }

    fn blend_layers(
        &mut self,
        a: Self::Task,
        b: Self::Task,
        w: Alpha,
    ) -> anyhow::Result<Self::Task> {
        Ok(self.1.blend_masked(a, b, w))
    }

    fn interpolate(
        &mut self,
        a: Self::Task,
        b: Self::Task,
        w: Alpha,
    ) -> anyhow::Result<Self::Task> {
        Ok(self.1.interpolate(a, b, w))
    }
}

pub trait PoseNode: GraphNode {
    fn sample(&self, tasks: &mut BlendTree, graph: &Graph)
        -> anyhow::Result<Option<BlendSampleId>>;
}

pub trait PoseParent: GraphNode {
    fn apply_parent(
        &self,
        tasks: &mut BlendTree,
        child_task: BlendSampleId,
        graph: &Graph,
    ) -> anyhow::Result<BlendSampleId>;

    fn as_pose(&self) -> Option<&dyn PoseNode>;
}

pub trait PoseGraph {
    fn sample_pose(
        &self,
        tasks: &mut BlendTree,
        index: NodeIndex,
    ) -> anyhow::Result<Option<BlendSampleId>>;
    fn pose_parent(&self, index: NodeIndex) -> Option<&dyn PoseParent>;
}

impl PoseGraph for Graph {
    fn sample_pose(
        &self,
        tasks: &mut BlendTree,
        index: NodeIndex,
    ) -> anyhow::Result<Option<BlendSampleId>> {
        let Some(node) = self.try_get_node(index) else {
            return Ok(None);
        };

        if let Some(sampler) = node.as_pose() {
            sampler.sample(tasks, self)
        } else {
            Ok(None)
        }
    }

    fn pose_parent(&self, index: NodeIndex) -> Option<&dyn PoseParent> {
        self.try_get_node(index).and_then(|x| x.as_pose_parent())
    }
}

pub trait BlendTreeVisitor {
    fn visit(&mut self, tree: &BlendTree, sample: &BlendSample);
}
