//! Named seeded streams with complete serializable positions.

use alloc::boxed::Box;

use rand::seq::SliceRandom as _;
use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{Result, invalid};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RandomWire", into = "RandomWire")]
pub(crate) struct Random(Box<ChaCha12Rng>);

#[derive(Serialize, Deserialize)]
#[serde(tag = "algorithm", content = "state", deny_unknown_fields)]
enum RandomWire {
    #[serde(rename = "chacha12-sha256-rand09-v1")]
    ChaCha12(ChaChaState),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaChaState {
    seed: [u8; 32],
    stream: u64,
    // Two limbs preserve the 68-bit cursor through JSON.
    word_pos: [u64; 2],
}

impl From<Random> for RandomWire {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "The two limbs retain every cursor bit."
    )]
    fn from(random: Random) -> Self {
        // rand_chacha 0.10 no longer wraps the cursor at the stream end itself.
        let position = random.0.get_word_pos() & ((1_u128 << 68_u32) - 1);
        Self::ChaCha12(ChaChaState {
            seed: random.0.get_seed(),
            stream: random.0.get_stream(),
            word_pos: [position as u64, (position >> 64) as u64],
        })
    }
}

impl TryFrom<RandomWire> for Random {
    type Error = crate::EngineFailure;

    fn try_from(wire: RandomWire) -> Result<Self> {
        let RandomWire::ChaCha12(state) = wire;
        let [low, high] = state.word_pos;
        if high > 15 {
            return Err(invalid(
                "random word position exceeds the 68-bit ChaCha stream",
            ));
        }
        let mut rng = ChaCha12Rng::from_seed(state.seed);
        rng.set_stream(state.stream);
        rng.set_word_pos(u128::from(low) | (u128::from(high) << 64));
        Ok(Self(Box::new(rng)))
    }
}

impl Random {
    pub(crate) fn new(seed: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"sve-engine/chacha12-sha256-rand09-v1\0");
        hash.update(seed.as_bytes());
        Self(Box::new(ChaCha12Rng::from_seed(hash.finalize().into())))
    }

    pub(crate) fn bounded(&mut self, bound: u64) -> Result<u64> {
        if bound == 0 {
            return Err(invalid("random range must be nonempty"));
        }
        Ok(self.0.random_range(0..bound))
    }

    pub(crate) fn shuffle<T>(&mut self, items: &mut [T]) {
        items.shuffle(self.0.as_mut());
    }
}

#[cfg(test)]
mod tests;
