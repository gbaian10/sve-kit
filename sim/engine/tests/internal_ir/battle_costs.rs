use super::*;
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

pub(super) fn object(
    observation: &native::Observation,
    seat: Viewer,
    zone: native::ObservedZone,
) -> ObjectHandle {
    board_fixtures::object(observation, seat, zone).unwrap()
}
