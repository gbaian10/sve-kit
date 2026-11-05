#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these complete printed discard scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"},
        {"id":"draw1","card":"BP01-171"},
        {"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP20-111"}]);
    initial
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "discard-return-light",
    )
    .unwrap()
}

fn activation(number: &str) -> AbilityId {
    selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "These printed cards ask only payment, one material, a target, or May."
)]
fn finish(game: &mut Game, mut play: CardPlay, preferred: Option<&str>, accept: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Integer(bounds) => {
                assert_eq!(bounds.minimum, Nat(0), "the printed May permits declining");
                assert_eq!(bounds.maximum, Some(Nat(1)), "the printed May permits one acceptance");
                Answer::Integer(Nat(u64::from(accept)))
            }
            InputDomain::CostMaterials(domain) | InputDomain::Targets(domain) => {
                assert_eq!(domain.cardinality.minimum, Nat(1), "the printed cost or target requires one member");
                assert_eq!(domain.cardinality.maximum, Some(Nat(1)), "the printed cost or target accepts only one member");
                let member = domain
                    .members
                    .iter()
                    .find(|member| {
                        preferred.is_none_or(|number| member.card.as_deref() == Some(number))
                    })
                    .unwrap();
                if matches!(question.domain, InputDomain::CostMaterials(_)) {
                    Answer::CostMaterials(vec![member.handle])
                } else {
                    Answer::Targets(vec![member.handle])
                }
            }
            _ => panic!("unexpected printed choice: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)), "the complete printed action enters or resolves");
    finish_checks(game);
}

fn pending(game: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("a printed pending occurrence");
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn hellflames(view: &Value, zone: &str) -> usize {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["card"] == "BP02-T06")
        .count()
}

fn replay_material(game: &mut Game, material: &str) {
    let replayed = game.play_internal_card("material", PlayerId { seat: 1 }).unwrap();
    finish(game, replayed, Some("BP20-111"), true);
    let replay = game.projection(View::Referee).unwrap();
    if material == "BP10-066" {
        assert_eq!(replay["P1"]["pp"]["current"], 3_u8, "Sun Dragon pays its original three PP from EX");
        assert!(replay["P1"]["field"].as_array().unwrap().contains(&json!("material")), "the actual returned Dragon enters the field");
    } else {
        assert_eq!(replay["P1"]["pp"]["current"], 4_u8, "Fist pays its original two PP from EX");
        assert_eq!(replay["P2"]["cemetery"], json!(["enemy"]), "Fist deals its printed three damage to the formal target");
    }
}

#[test]
fn flute_and_return_prints_pay_real_costs_create_tokens_and_replay_the_complete_bodies() {
    for (flute, material) in [
        ("BP02-054", "BP10-066"),
        ("BP02-054", "BP04-071"),
        ("BP02-SL12", "BP04-P19"),
    ] {
        for accept in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"ready","card":flute}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"played","card":flute},
                {"id":"material","card":material},
                {"id":"discarded-flute","card":flute}
            ]);
            let mut game = board(&initial);
            let entered = game
                .play_internal_card("played", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, entered, None, true);
            let fanfare = pending(&mut game, flute, 1);
            finish(&mut game, fanfare, None, true);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["pp"]["current"], 6_u8);
            assert_eq!(before["P1"]["hand"], json!(["material", "discarded-flute", "draw0"]));
            assert_eq!(hellflames(&before, "field"), 1);
            let token = before["P1"]["field"].as_array().unwrap().iter().find(|id| {
                before["objects"][id.as_str().unwrap()]["card"] == "BP02-T06"
            }).unwrap().as_str().unwrap();
            assert_eq!(before["objects"][token]["power"], 4_u8);
            assert_eq!(before["objects"][token]["hp"], 3_u8);
            assert!(before["objects"][token]["keywords"].as_array().unwrap().contains(&json!("突進")));
            let activated = game
                .activate_internal_card("played", activation(flute), PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, activated, Some(material), true);
            let returning = pending(&mut game, material, 1);
            finish(&mut game, returning, None, accept);
            let moved = game.projection(View::Referee).unwrap();
            assert_eq!(moved["objects"]["played"]["acted"], true);
            assert_eq!(hellflames(&moved, "ex"), 1);
            assert_eq!(moved["P1"]["leader"]["life"], 15_u8, "the hand or EX Sun Dragon does not observe its own discard");
            assert_eq!(moved["P1"]["cemetery"], if accept { json!([]) } else { json!(["material"]) });
            if accept {
                assert!(moved["P1"]["ex"].as_array().unwrap().contains(&json!("material")));
                replay_material(&mut game, material);
            }
            let second = game.activate_internal_card("ready", activation(flute), PlayerId { seat: 1 }).unwrap();
            finish(&mut game, second, Some(flute), true);
            let flute_discarded = pending(&mut game, flute, 1);
            finish(&mut game, flute_discarded, None, true);
            if accept && material == "BP10-066" {
                let heal = pending(&mut game, material, 2);
                finish(&mut game, heal, None, true);
            }
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(hellflames(&after, "ex"), 3, "one actual discard triggers both Flute's paid activation and its discarded material");
            assert_eq!(after["objects"]["ready"]["acted"], true);
            assert_eq!(after["P1"]["leader"]["life"], if accept && material == "BP10-066" { 18_u8 } else { 15_u8 });
            assert_eq!(after["P1"]["deck"], json!(["draw1", "tail"]));
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn every_supremacy_print_requires_its_own_mazelbeine_then_pays_one_from_ex_and_draws_two() {
    for number in ["BP20-120", "BP20-P59"] {
        for present in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = if present {
                json!([{"id":"flute","card":"BP02-054"},{"id":"named","card":"BP20-111"}])
            } else {
                json!([{"id":"flute","card":"BP02-054"}])
            };
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            let mut game = board(&initial);
            let activated = game.activate_internal_card("flute", activation("BP02-054"), PlayerId { seat: 1 }).unwrap();
            finish(&mut game, activated, Some(number), true);
            let discarded = pending(&mut game, number, 1);
            finish(&mut game, discarded, None, true);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["cemetery"], if present { json!([]) } else { json!(["source"]) }, "the opponent's Mazelbeine cannot satisfy the own-field condition");
            if present {
                assert!(before["P1"]["ex"].as_array().unwrap().contains(&json!("source")));
                let replayed = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
                finish(&mut game, replayed, None, true);
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["P1"]["pp"]["current"], 9_u8, "the three-cost spell pays one only when played from EX");
                assert_eq!(after["P1"]["hand"], json!(["draw0", "draw1"]));
                assert_eq!(after["P1"]["deck"], json!(["tail"]));
                assert_eq!(after["P1"]["cemetery"], json!(["source"]));
            }
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}
