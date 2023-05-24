use std::ops::{Mul, MulAssign};

use glam::{Mat4, Quat, Vec3, Vec3A, Affine3A};

use crate::{CHARACTER_FORWARD, CHARACTER_RIGHT, CHARACTER_UP};

#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub translation: Vec3A,
    pub rotation: Quat,
    pub scale: Vec3A,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3A::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3A::ONE,
    };
    
    /// Blender perspective
    pub fn character_up(&self) -> Vec3A {
        self.rotation * CHARACTER_UP
    }

    /// Blender perspective
    pub fn character_right(&self) -> Vec3A {
        self.rotation * CHARACTER_RIGHT
    }

    /// Blender perspective
    pub fn character_forward(&self) -> Vec3A {
        self.rotation * CHARACTER_FORWARD
    }

    pub fn inverse(&self) -> Transform {
        let unsanitized_scale = self.scale.recip();
        let scale = Vec3A::select(
            unsanitized_scale.is_nan_mask(),
            Vec3A::ZERO,
            unsanitized_scale,
        );
        let rotation = self.rotation.inverse();
        let translation = -(rotation * (self.translation * scale));
        Transform {
            translation,
            rotation,
            scale,
        }
    }

    pub fn transform_point3a(&self, point: Vec3A) -> Vec3A {
        self.translation + self.rotation * (point * self.scale)
    }

    pub fn transform_vector3a(&self, vector: Vec3A) -> Vec3A {
        self.rotation * vector
    }

    pub fn transform_point3(&self, point: Vec3) -> Vec3 {
        self.transform_point3a(point.into()).into()
    }

    pub fn transform_vector3(&self, vector: Vec3) -> Vec3 {
        self.transform_vector3a(vector.into()).into()
    }

    pub fn lerp(&self, rhs: &Transform, s: f32) -> Self {
        let translation = self.translation.lerp(rhs.translation, s);
        let rotation = self.rotation.slerp(rhs.rotation, s);
        let scale = self.scale.lerp(rhs.scale, s);

        Self {
            translation,
            rotation,
            scale,
        }
    }
}

impl From<Mat4> for Transform {
    fn from(value: Mat4) -> Self {
        let (scale, rotation, translation) = value.to_scale_rotation_translation();
        Self {
            translation: translation.into(),
            rotation,
            scale: scale.into(),
        }
    }
}

impl From<Affine3A> for Transform {
    fn from(value: Affine3A) -> Self {
        let (scale, rotation, translation) = value.to_scale_rotation_translation();
        Self {
            translation: translation.into(),
            rotation,
            scale: scale.into(),
        }
    }
}

impl Mul<Transform> for Transform {
    type Output = Transform;

    fn mul(self, rhs: Transform) -> Self::Output {
        let rotation = (self.rotation * rhs.rotation).normalize();
        let translation = rhs.transform_point3a(self.translation);
        let scale = self.scale * rhs.scale;
        Self::Output {
            translation,
            rotation,
            scale,
        }
    }
}

impl MulAssign<Transform> for Transform {
    fn mul_assign(&mut self, rhs: Transform) {
        self.rotation = (self.rotation * rhs.rotation).normalize();
        self.translation = rhs.transform_point3a(self.translation);
        self.scale = self.scale * rhs.scale;
    }
}


