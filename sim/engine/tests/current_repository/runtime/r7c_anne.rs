#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The printed Anne procedures must expose only their declared choices."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PRINTINGS: [&str; 4] = ["BP13-036", "BP13-SL08", "BP21-PR07", "BP21-PR16"];
const ACADEMY: [&str; 5] = ["BP13-042", "BP18-053", "BP21-041", "BP21-047", "BP21-048"];
const PLAYER: PlayerId = PlayerId { seat: 1 };

fn initial(number: &str, zone: &str, grave: usize, pp: u8) -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
    position["players"]["P1"]["pp"] = json!({"current":pp,"max":6_u8});
    position["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    position["players"]["P1"]["zones"]["cemetery"] = json!(
        ACADEMY
            .iter()
            .cycle()
            .take(grave)
            .enumerate()
            .map(|(n, card)| json!({"id":format!("grave-{n}"),"card":card}))
            .collect::<Vec<_>>()
    );
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
    ]);
    position
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed Anne choice")
    };
    question
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay, accept: bool, fuel: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.members.len(), 1, "only the Academy fuel is eligible");
                assert_eq!(
                    domain.members[0].card.as_deref(),
                    Some("BP18-053"),
                    "the non-Academy hand card is excluded"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            other => panic!("unexpected Anne Fanfare choice {other:?}"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    if accept && fuel {
        assert!(
            matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
            "a payable accepted printed play completes"
        );
    } else {
        assert!(
            matches!(play, CardPlay::Cancelled),
            "declined or impossible required Fanfare costs cancel its occurrence"
        );
    }
    checks(board);
}

#[test]
fn anne_all_printings_pay_normal_blast_cost_and_return_it_face_up() {
    for (index, number) in PRINTINGS.into_iter().enumerate() {
        for (grave, pp, accept, face_up) in [
            (14, 3_u8, true, false),
            (15, 2, true, false),
            (15, 3, true, false),
            (15, 3, false, false),
            (15, 3, true, true),
        ] {
            let mut position = initial(number, "field", grave, pp);
            let blast = if index % 2 == 0 {
                "BP13-039"
            } else {
                "BP13-SL11"
            };
            position["players"]["P1"]["zones"]["evolve_deck"] = json!([
                {"id":"blast","card":blast,"face_up":face_up},
                {"id":"used","card":blast,"face_up":true},
                {"id":"wrong","card":"BP07-105","face_up":false}
            ]);
            position["players"]["P2"]["zones"]["evolve_deck"] =
                json!([{"id":"foreign","card":blast}]);
            let mut board = game(&position);
            let ability = selection().current[number].program.card().abilities[1]
                .node
                .id;
            let before = format!("{board:?}");
            let play = board.activate_internal_card("source", ability, PLAYER);
            if grave < 15 {
                assert!(matches!(play.unwrap_err(), EngineFailure::IllegalPlay(_)));
                assert_eq!(
                    format!("{board:?}"),
                    before,
                    "a failed prerequisite pays no cost"
                );
                continue;
            }
            let q = awaiting(play.unwrap());
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("private Advance spell")
            };
            assert_eq!(domain.members.len(), usize::from(!face_up));
            assert!(board.internal_decision(View::P2).is_none());
            let handles = if accept && !face_up {
                vec![domain.members[0].handle]
            } else {
                vec![]
            };
            let result = resolved(
                board
                    .answer_internal_card(q.actor, q.token, Answer::Targets(handles))
                    .unwrap(),
            );
            let state = board.projection(View::Referee).unwrap();
            let played = pp >= 3 && accept && !face_up;
            assert_eq!(
                state["P1"]["pp"]["current"],
                if played { pp - 3 } else { pp }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if played { 12_u8 } else { 20_u8 }
            );
            assert_eq!(state["objects"]["source"]["acted"], true);
            assert_eq!(state["objects"]["blast"]["face_up"], played || face_up);
            assert_eq!(
                state["objects"]["blast"]["generation"],
                if played { 3_u8 } else { 0_u8 }
            );
            assert_eq!(state["P1"]["resolution"], json!([]));
            assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), grave);
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|f| matches!(f, Fact::CardPlayed { .. }))
                    .count(),
                usize::from(played)
            );
            checks(&mut board);
            let before_direct = format!("{board:?}");
            board.play_internal_card("blast", PLAYER).unwrap_err();
            assert_eq!(
                format!("{board:?}"),
                before_direct,
                "the ordinary play command still cannot play from evolve deck"
            );
        }
    }
}

#[test]
fn anne_all_printings_discard_only_academy_then_draw_and_create_the_real_spirit() {
    for number in PRINTINGS {
        for (grave, fuel, accept) in [
            (3_usize, true, true),
            (4, true, true),
            (4, false, true),
            (4, true, false),
        ] {
            let mut position = initial(number, "hand", grave, 4);
            position["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"fuel","card":if fuel {"BP18-053"} else {"BP01-171"}}),
                    json!({"id":"decoy","card":"BP01-171"}),
                ]);
            let mut board = game(&position);
            let play = board.play_internal_card("source", PLAYER).unwrap();
            finish(&mut board, play, true, true);
            let q = board.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("Anne Fanfare")
            };
            assert_eq!(options.len(), 1);
            let fanfare_play = board
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            finish(&mut board, fanfare_play, accept, fuel);
            assert!(board.internal_pending_choice().unwrap().is_none());
            let state = board.projection(View::Referee).unwrap();
            let paid = fuel && accept;
            assert_eq!(state["P1"]["pp"]["current"], 2_u8);
            assert_eq!(
                state["P1"]["deck"],
                if paid {
                    json!(["tail"])
                } else {
                    json!(["drawn", "tail"])
                }
            );
            assert_eq!(
                state["P1"]["cemetery"].as_array().unwrap().len(),
                grave + usize::from(paid)
            );
            assert!(
                state["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("decoy"))
            );
            assert_eq!(
                state["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fuel")),
                !paid,
                "a cancelled Fanfare keeps its fuel"
            );
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fuel")),
                paid,
                "only an actually paid cost discards the fuel"
            );
            let ex = state["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), usize::from(paid && grave == 4));
            if !ex.is_empty() {
                let token = &state["objects"][ex[0].as_str().unwrap()];
                assert_eq!(token["name"], "アンの大英霊");
                assert_eq!(token["power"], 4_u8);
                assert_eq!(token["hp"], 4_u8);
            }
        }
    }
}
