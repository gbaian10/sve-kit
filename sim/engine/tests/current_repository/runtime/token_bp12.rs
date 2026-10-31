#![expect(
    clippy::panic,
    reason = "These real-card scenarios require the printed resolution choice."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(number: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state
}

fn fanfare(game: &mut Game, number: &str) -> DecisionView {
    assert!(
        matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real one-PP Dual Angel enters before its fanfare resolves"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real fanfare occurrence");
    };
    assert_eq!(options.len(), 1, "one printed fanfare is pending");
    assert_eq!(
        options.first().unwrap().card,
        number,
        "the actual printing supplies the effect"
    );
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options.first().unwrap().handle),
        )
        .unwrap()
    else {
        panic!("choose Repair Mode or Tree during resolution");
    };
    choice
}

#[test]
fn bp12_104_printings_choose_one_complete_token_and_repair_can_be_played_from_ex() {
    for number in ["BP12-104", "BP12-P20", "PR-327"] {
        for (index, expected) in [(0_usize, "BP07-T02"), (1, "BP07-T03")] {
            let mut game = game(&position(number));
            let choice = fanfare(&mut game, number);
            let InputDomain::Modes {
                modes,
                cardinality,
                count,
            } = &choice.domain
            else {
                panic!("the printed either uses resolution modes");
            };
            assert_eq!(*count, Nat(1), "choose exactly one token name");
            assert_eq!(
                cardinality.minimum,
                Nat(1),
                "the required choice cannot be declined"
            );
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["P1"]["pp"]["current"], 9_u8,
                "printed one-PP payment precedes the choice"
            );
            assert_eq!(
                before["P1"]["ex"],
                json!([]),
                "no token exists before its name is chosen"
            );
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Modes(vec![modes[index]]),
                )
                .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            let ex = view["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1, "only the selected token reaches EX");
            let id = ex.first().unwrap().as_str().unwrap();
            assert_eq!(
                view["objects"][id]["card"], expected,
                "the real admitted token template is used"
            );
            assert_eq!(
                view["P2"]["ex"],
                json!([]),
                "the opponent receives no token"
            );
            if index == 0 {
                let repair = id.to_owned();
                game.resume_internal_card().unwrap();
                resolved(game.play_internal_card(&repair, actor()).unwrap());
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(
                    after["P1"]["leader"]["life"], 11_u8,
                    "the complete Repair Mode heals its controller"
                );
                assert_eq!(
                    after["P1"]["pp"]["current"], 8_u8,
                    "playing the real token from EX pays its printed PP"
                );
                assert_eq!(
                    after["P1"]["ex"],
                    json!([]),
                    "the played spell token leaves EX"
                );
                assert!(
                    !after["P1"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(repair)),
                    "the spell token is erased by the normal token rule"
                );
            }
        }
    }
}
