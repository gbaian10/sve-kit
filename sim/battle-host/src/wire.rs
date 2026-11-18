//! Channel messages between the local host and its seats.
//!
//! Every message is the battle contract's (`sve-battle-contract`); this module only names the
//! seats the Worker routes by and maps them to native viewers.

pub use sve_battle_contract::{
    Answer, BattleInput, BattleUpdate, ClientMessage, Command, HostError, HostErrorReason,
    HostMessage, Nat, QuestionId, Receipt, ReceiptResult, Rejection, RequestId, Seat, Submit,
};
use sve_engine::game::player::Viewer;

/// The seat named on the wire, if any.
#[must_use]
pub fn seat_named(name: &str) -> Option<Seat> {
    match name {
        "p1" => Some(Seat::P1),
        "p2" => Some(Seat::P2),
        _ => None,
    }
}

/// The wire seat of a native viewer.
#[must_use]
pub const fn seat_of(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

/// The native viewer of a wire seat.
#[must_use]
pub const fn viewer_of(seat: Seat) -> Viewer {
    match seat {
        Seat::P1 => Viewer::P1,
        Seat::P2 => Viewer::P2,
    }
}
