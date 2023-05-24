use glam::{Mat3A, Vec3, Vec3A};
use serde_derive::{Deserialize, Serialize};

pub fn lcm_or_one(a: u32, b: u32) -> u32 {
    fn steins_gcd(mut m: u32, mut n: u32) -> u32 {
        let shift = (m | n).trailing_zeros();
        m >>= m.trailing_zeros();
        n >>= n.trailing_zeros();
        while m != n {
            if m > n {
                m -= n;
                m >>= m.trailing_zeros();
            } else {
                n -= m;
                n >>= n.trailing_zeros();
            }
        }
        m << shift
    }
    if a <= 1 {
        b.max(1)
    } else if b <= 1 {
        a
    } else {
        a * (b / steins_gcd(a, b))
    }
}

/// Blender perspective
pub const CHARACTER_RIGHT: Vec3A = Vec3A::NEG_X;
/// Blender perspective
pub const CHARACTER_FORWARD: Vec3A = Vec3A::NEG_Y;
/// Blender perspective
pub const CHARACTER_UP: Vec3A = Vec3A::Z;

/// Blender perspective
pub const CHARACTER_BASIS: Mat3A =
    Mat3A::from_cols(CHARACTER_RIGHT, CHARACTER_FORWARD, CHARACTER_UP);

/// Blender perspective
pub mod character_vec3 {
    use glam::Vec3;
    /// Blender perspective
    pub const RIGHT: Vec3 = Vec3::NEG_X;
    /// Blender perspective
    pub const LEFT: Vec3 = Vec3::X;
    /// Blender perspective
    pub const FORWARD: Vec3 = Vec3::NEG_Y;
    /// Blender perspective
    pub const BACK: Vec3 = Vec3::Y;
    /// Blender perspective
    pub const UP: Vec3 = Vec3::Z;
    /// Blender perspective
    pub const DOWN: Vec3 = Vec3::NEG_Z;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Projection {
    Length,
    Horizontal,
    Vertical,
    Forward,
    Back,
    Right,
    Left,
    Up,
    Down,
}

impl Projection {
    /// Blender perspective
    pub fn character_projected(self, vec: Vec3) -> f32 {
        use character_vec3::*;
        match self {
            Projection::Length => vec.length(),
            Projection::Horizontal => vec.truncate().length(),
            Projection::Vertical => vec.z,
            Projection::Forward => vec.dot(FORWARD),
            Projection::Back => vec.dot(BACK),
            Projection::Right => vec.dot(RIGHT),
            Projection::Left => vec.dot(LEFT),
            Projection::Up => vec.dot(UP),
            Projection::Down => vec.dot(DOWN),
        }
    }
}

#[cfg(test)]
mod test {
    use super::lcm_or_one;

    #[test]
    fn test_lcm() {
        assert_eq!(lcm_or_one(3, 4), 12);
        assert_eq!(lcm_or_one(0, 0), 1);
        assert_eq!(lcm_or_one(2, 0), 2);
        assert_eq!(lcm_or_one(0, 2), 2);
        assert_eq!(lcm_or_one(2, 2), 2);
        assert_eq!(lcm_or_one(u32::MAX, u32::MAX), u32::MAX);
    }
}
