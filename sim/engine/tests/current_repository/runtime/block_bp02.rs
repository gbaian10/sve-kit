use super::event_evolved::{evolved, initial_game, resolve_observer};
use super::*;

#[test]
fn bp02_016_creates_a_real_fairy_wisp_in_ex_only_for_its_controllers_native_evolution() {
    for side in ["P1", "P2"] {
        let mut game = evolved("BP02-016", side);
        resolve_observer(&mut game, "BP02-016", side == "P1");
        let state = game.projection(View::Referee).unwrap();
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(
            ex.len(),
            usize::from(side == "P1"),
            "only an allied evolution creates one native token in EX"
        );
        if side == "P1" {
            let wisp = &state["objects"][ex[0].as_str().unwrap()];
            assert_eq!(
                wisp["name"], "フェアリーウィスプ",
                "the created object comes from the real token template"
            );
            assert_eq!(wisp["power"], 1_u8);
            assert_eq!(wisp["hp"], 1_u8);
        }
    }
}

#[test]
fn bp02_064_gains_only_two_power_for_an_allied_native_evolution() {
    for side in ["P1", "P2"] {
        let before = initial_game("BP02-064", side)
            .projection(View::Referee)
            .unwrap()["objects"]["observer"]
            .clone();
        let mut game = evolved("BP02-064", side);
        resolve_observer(&mut game, "BP02-064", side == "P1");
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["observer"]["power"],
            before["power"].as_u64().unwrap() + if side == "P1" { 2 } else { 0 },
            "only its controller's evolution grants two power"
        );
        assert_eq!(
            after["objects"]["observer"]["hp"], before["hp"],
            "the ability leaves health unchanged"
        );
    }
}
