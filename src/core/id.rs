use serde_derive::{Deserialize, Serialize};

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct ConstId<const N: u64, const S: u64>(pub u64);

impl<const N: u64, const S: u64> ConstId<N, S> {
    pub const NAMESPACE: [u64; 4] = const_wyhash::make_secret(N);
    pub const SEED: u64 = S;

    pub const fn from_str(value: &str) -> Self {
        let bytes = value.as_bytes();
        let (a, b, seed) = const_wyhash::wyhash_core(bytes, Self::SEED, Self::NAMESPACE);
        let hash = const_wyhash::wyhash_finish(a, b, seed, bytes.len() as u64, Self::NAMESPACE[1]);
        Self(hash)
    }
}

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Id(pub ConstId<0, 0x2614_9574_d5fa_c1fe>);

impl Id {
    pub const EMPTY: Self = Self::from_str("");

    pub const fn from_bits(bits: u64) -> Self {
        Self(ConstId(bits))
    }

    pub const fn to_bits(self) -> u64 {
        self.0 .0
    }

    pub const fn is_empty(self) -> bool {
        self.0 .0 == Self::EMPTY.0 .0
    }

    pub const fn from_str(value: &str) -> Self {
        if value.is_empty() {
            Self(ConstId(0))
        } else {
            Self(ConstId::from_str(value))
        }
    }
}

#[cfg(test)]
mod test {
    use super::Id;

    #[test]
    fn test_id_case_sensitive() {
        const UPPERCASE_A: Id = Id::from_str("A");
        const LOWERCASE_A: Id = Id::from_str("a");
        assert_ne!(UPPERCASE_A, LOWERCASE_A);
    }

    fn get_runtime_id(s: &str) -> Id {
        Id::from_str(s)
    }

    #[test]
    fn test_empty() {
        let id = get_runtime_id("");

        assert_eq!(id, Id::EMPTY);
        assert_eq!(id.to_bits(), 0)
    }
}

pub mod const_wyhash {
    // constified version of wyhash-rs
    // https://github.com/eldruin/wyhash-rs/blob/8707fbe33bcd8354819712a93a4457cc83c367c2/src/final3/functions.rs

    pub const P0: u64 = 0xa076_1d64_78bd_642f;
    pub const P1: u64 = 0xe703_7ed1_a0b4_28db;

    const fn wymum(a: u64, b: u64) -> u64 {
        let r = (a as u128) * (b as u128);
        ((r >> 64) ^ r) as u64
    }

    const fn read64(bytes: &[u8], i: usize) -> u64 {
        u64::from_le_bytes([
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3],
            bytes[i + 4],
            bytes[i + 5],
            bytes[i + 6],
            bytes[i + 7],
        ])
    }

    const fn read32(bytes: &[u8], i: usize) -> u64 {
        u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as u64
    }

    #[inline]
    const fn read_up_to_24(bytes: &[u8]) -> u64 {
        (bytes[0] as u64) << 16
            | ((bytes[bytes.len() >> 1]) as u64) << 8
            | (bytes[bytes.len() - 1]) as u64
    }

    /// Generate a hash for the input data and seed
    pub const fn wyhash(bytes: &[u8], seed: u64, secret: [u64; 4]) -> u64 {
        let seed = seed ^ secret[0];
        let (a, b, seed) = wyhash_core(bytes, seed, secret);
        wyhash_finish(a, b, seed, bytes.len() as u64, secret[1])
    }

    #[inline]
    pub const fn wyhash_core(bytes: &[u8], seed: u64, secret: [u64; 4]) -> (u64, u64, u64) {
        let (mut a, mut b) = (0, 0);
        let mut seed = seed;
        let length = bytes.len();
        if length <= 16 {
            if length >= 4 {
                a = read32(bytes, 0) << 32 | read32(bytes, (length >> 3) << 2);
                b = read32(bytes, length - 4) << 32
                    | read32(bytes, length - 4 - ((length >> 3) << 2));
            } else if length > 0 {
                a = read_up_to_24(bytes);
            }
        } else {
            let mut index = length;
            let mut start = 0;
            if length > 48 {
                let mut see1 = seed;
                let mut see2 = seed;
                while index > 48 {
                    seed = wymum(
                        read64(bytes, start) ^ secret[1],
                        read64(bytes, start + 8) ^ seed,
                    );
                    see1 = wymum(
                        read64(bytes, start + 16) ^ secret[2],
                        read64(bytes, start + 24) ^ see1,
                    );
                    see2 = wymum(
                        read64(bytes, start + 32) ^ secret[3],
                        read64(bytes, start + 40) ^ see2,
                    );
                    index -= 48;
                    start += 48;
                }
                seed ^= see1 ^ see2;
            }

            while index > 16 {
                seed = wymum(
                    read64(bytes, start) ^ secret[1],
                    read64(bytes, start + 8) ^ seed,
                );
                index -= 16;
                start += 16
            }

            a = read64(bytes, length - 16);
            b = read64(bytes, length - 8);
        }
        (a, b, seed)
    }

    #[inline]
    pub const fn wyhash_finish(a: u64, b: u64, seed: u64, length: u64, secret1: u64) -> u64 {
        wymum(secret1 ^ length, wymum(a ^ secret1, b ^ seed))
    }

    /// Generate new secret for wyhash
    pub const fn make_secret(seed: u64) -> [u64; 4] {
        let c = [
            15_u8, 23, 27, 29, 30, 39, 43, 45, 46, 51, 53, 54, 57, 58, 60, 71, 75, 77, 78, 83, 85,
            86, 89, 90, 92, 99, 101, 102, 105, 106, 108, 113, 114, 116, 120, 135, 139, 141, 142,
            147, 149, 150, 153, 154, 156, 163, 165, 166, 169, 170, 172, 177, 178, 180, 184, 195,
            197, 198, 201, 202, 204, 209, 210, 212, 216, 225, 226, 228, 232, 240,
        ];
        let mut secret = [0_u64; 4];
        let mut seed = seed;

        let mut i = 0usize;
        while i < secret.len() {
            'search: loop {
                secret[i] = 0;
                let mut j = 0usize;
                while j < 64 {
                    let (a, rng) = wyrng(seed);
                    seed = a;
                    let ndx = (rng % c.len() as u64) as usize;
                    secret[i] |= (c[ndx] as u64) << j;
                    j += 8;
                }
                if secret[i] % 2 == 0 {
                    continue;
                }

                let mut j = 0usize;
                let b = secret[i];
                while j < i {
                    let a = secret[j];
                    if (a ^ b).count_ones() != 32 {
                        continue 'search;
                    }
                    j += 1;
                }

                break;
            }
            i += 1;
        }
        secret
    }

    /// Pseudo-Random Number Generator (PRNG)
    ///
    /// Note that the input seed is updated
    pub const fn wyrng(mut seed: u64) -> (u64, u64) {
        seed = seed.wrapping_add(P0);
        (seed, wymum(seed, seed ^ P1))
    }
}
