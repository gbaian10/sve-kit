//! Random self-play over the local battle host.
//!
//! Decks are drawn at random from every admitted printing, or by weight from a deck list.
//! Both seats are wire players: every decision is a contract [`Answer`](sve_battle_contract::Answer)
//! chosen at random inside the domain the question states, so a match exercises the same
//! projection, decoding and rules path as the browser Worker. A seed names the decks and
//! every answer; the record of a match replays without this crate's generators.

#![expect(
    clippy::multiple_crate_versions,
    reason = "The engine's dependencies already pull two syn and getrandom generations."
)]

extern crate alloc;

pub mod answer;
pub mod arena;
pub mod check;
pub mod cli;
pub mod lineup;
pub mod play;
pub mod pool;
pub mod published;
pub mod report;
pub mod usage;
