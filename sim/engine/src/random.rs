//! Versioned seeded streams; old numeric saves retain their original sequence.

use alloc::boxed::Box;

use rand::seq::SliceRandom as _;
use rand::{Rng as _, RngCore as _, SeedableRng as _};
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{Result, invalid};

/// Algorithm selection for new games and reproducible seed-based reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomAlgorithm {
    /// Original FNV-1a seed fold, `SplitMix64` words and legacy sampling.
    Legacy,
    /// SHA-256 seed expansion, `ChaCha12` and rand 0.9.5 unbiased sampling.
    ChaCha12V1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RandomWire", into = "RandomWire")]
pub(crate) enum Random {
    Legacy(u64),
    ChaCha12(Box<ChaCha12Rng>),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RandomWire {
    Legacy(u64),
    Versioned(Box<VersionedWire>),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "algorithm", content = "state", deny_unknown_fields)]
enum VersionedWire {
    #[serde(rename = "chacha12-sha256-rand09-v1")]
    ChaCha12(ChaChaState),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaChaState {
    seed: [u8; 32],
    stream: u64,
    // Two limbs preserve the 68-bit cursor through JSON and Serde's untagged enum.
    word_pos: [u64; 2],
}

impl From<Random> for RandomWire {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "The two limbs retain every cursor bit."
    )]
    fn from(random: Random) -> Self {
        match random {
            Random::Legacy(state) => Self::Legacy(state),
            Random::ChaCha12(rng) => {
                let position = rng.get_word_pos();
                Self::Versioned(Box::new(VersionedWire::ChaCha12(ChaChaState {
                    seed: rng.get_seed(),
                    stream: rng.get_stream(),
                    word_pos: [position as u64, (position >> 64) as u64],
                })))
            }
        }
    }
}

impl TryFrom<RandomWire> for Random {
    type Error = crate::EngineFailure;

    fn try_from(wire: RandomWire) -> Result<Self> {
        match wire {
            RandomWire::Legacy(state) => Ok(Self::Legacy(state)),
            RandomWire::Versioned(versioned) => {
                let VersionedWire::ChaCha12(state) = *versioned;
                let [low, high] = state.word_pos;
                if high > 15 {
                    return Err(invalid(
                        "random word position exceeds the 68-bit ChaCha stream",
                    ));
                }
                let mut rng = ChaCha12Rng::from_seed(state.seed);
                rng.set_stream(state.stream);
                rng.set_word_pos(u128::from(low) | (u128::from(high) << 64));
                Ok(Self::ChaCha12(Box::new(rng)))
            }
        }
    }
}

impl Random {
    pub(crate) fn new(seed: &str, algorithm: RandomAlgorithm) -> Self {
        match algorithm {
            RandomAlgorithm::Legacy => {
                Self::Legacy(seed.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |s, b| {
                    (s ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
                }))
            }
            RandomAlgorithm::ChaCha12V1 => {
                let mut hash = Sha256::new();
                hash.update(b"sve-engine/chacha12-sha256-rand09-v1\0");
                hash.update(seed.as_bytes());
                Self::ChaCha12(Box::new(ChaCha12Rng::from_seed(hash.finalize().into())))
            }
        }
    }

    pub(crate) fn next(&mut self) -> u64 {
        match self {
            Self::Legacy(state) => {
                *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
                let mut n = *state;
                n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                n ^ (n >> 31)
            }
            Self::ChaCha12(rng) => rng.next_u64(),
        }
    }

    pub(crate) fn bounded(&mut self, bound: u64) -> Result<u64> {
        let threshold = bound
            .wrapping_neg()
            .checked_rem(bound)
            .ok_or_else(|| invalid("random range must be nonempty"))?;
        if let Self::ChaCha12(rng) = self {
            return Ok(rng.random_range(0..bound));
        }
        // Numeric saves must retain rejection and word consumption, not just the PRNG.
        loop {
            let value = self.next();
            if value >= threshold {
                return value
                    .checked_rem(bound)
                    .ok_or_else(|| invalid("random range must be nonempty"));
            }
        }
    }

    pub(crate) fn shuffle<T>(&mut self, items: &mut [T]) {
        if let Self::ChaCha12(rng) = self {
            items.shuffle(rng.as_mut());
            return;
        }
        // Legacy shuffles used modulo instead of the bounded sampler above.
        for i in (1..items.len()).rev() {
            if let Ok(bound) = u64::try_from(i.saturating_add(1))
                && let Ok(j) = usize::try_from(self.next().checked_rem(bound).unwrap_or_default())
            {
                items.swap(i, j);
            }
        }
    }
}

#[cfg(test)]
mod tests;
