#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed-card scenarios."
)]
use super::*;

fn companion_game(initial: &Value, kind: &str, trait_name: &str) -> Game {
    let (mut snapshot, keywords, mut input) = inputs().clone();
    let mut face = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|card| card["number"] == "BP01-037")
        .unwrap()["faces"][0]
        .clone();
    face["name"] = json!("trait-companion");
    face["card_type"] = json!(kind);
    face["traits"] = json!([trait_name]);
    face["text"] = Value::Null;
    face["sections"] = json!([]);
    if kind == "アミュレット" {
        face["power"] = json!("-");
        face["hp"] = json!("-");
    }
    snapshot.push_str(&json!({"number":"trait-companion","faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!(["trait-companion.yaml", "format: 1\nkind: effect_set\ncards:\n  trait-companion:\n    status: complete\n    abilities: []\n"]));
    let mut game = game_from_selection(initial, &snapshot, &keywords, &input);
    game.load_internal_card(
        "trait-companion",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn position(card: &str, seat: &str) -> Value {
    let mut initial = draw_position();
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"][seat]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn add_companion(initial: &mut Value, seat: &str, zone: &str) {
    if initial["players"][seat]["zones"][zone].is_null() {
        initial["players"][seat]["zones"][zone] = json!([]);
    }
    initial["players"][seat]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"companion","card":"trait-companion"}));
}

fn enter(game: &mut Game, seat: &str) {
    assert!(
        matches!(
            game.play_internal_card(
                "source",
                PlayerId {
                    seat: seat_number(seat)
                }
            )
            .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters for its printed cost"
    );
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending trigger");
    };
    assert_eq!(options.len(), 1_usize, "one printed trigger is pending");
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the trigger"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn target(game: &mut Game, play: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target precedes the condition");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("opposing follower target");
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP04-117"))
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap_err();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![handle]),
    )
    .unwrap()
}

#[test]
fn other_trait_field_bp14_047_requires_another_allied_wizard_follower_after_declaring_target() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for (owner, zone, kind, trait_name, expected) in [
            (None, "field", "フォロワー", "魔法使い", false),
            (Some(seat), "field", "フォロワー", "魔法使い", true),
            (Some(opponent), "field", "フォロワー", "魔法使い", false),
            (Some(seat), "hand", "フォロワー", "魔法使い", false),
            (Some(seat), "ex", "フォロワー", "魔法使い", false),
            (Some(seat), "cemetery", "フォロワー", "魔法使い", false),
            (Some(seat), "field", "フォロワー", "童話", false),
            (Some(seat), "field", "アミュレット", "魔法使い", false),
        ] {
            let mut initial = position("BP14-047", seat);
            if let Some(owner) = owner {
                add_companion(&mut initial, owner, zone);
            }
            let mut game = companion_game(&initial, kind, trait_name);
            enter(&mut game, seat);
            let play = trigger(&mut game, "BP14-047");
            let result = target(&mut game, play);
            resolved(result);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if expected { 3_u8 } else { 0_u8 }
            );
            assert_eq!(state[seat]["pp"]["current"], 7_u8);
        }
    }
}

#[test]
fn other_trait_field_bp14_048_real_evolution_targets_before_drawing_and_discarding() {
    for enemy in [false, true] {
        let mut initial = position("BP14-047", "P1");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-047"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-048"}]);
        if !enemy {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut game = game(&initial);
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        let play = trigger(&mut game, "BP14-048");
        if enemy {
            let CardPlay::Awaiting(discard) = target(&mut game, play) else {
                panic!("draw precedes a private discard choice");
            };
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["hand"], json!(["first"]));
            assert_eq!(before["objects"]["enemy"]["damage"], 3_u8);
            let InputDomain::Targets(domain) = &discard.domain else {
                panic!("private hand discard");
            };
            assert_eq!(domain.members.len(), 1_usize);
            resolved(
                game.answer_internal_card(
                    discard.actor,
                    discard.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
        } else {
            assert!(
                matches!(play, CardPlay::Cancelled),
                "Q1908: no target means no draw or discard"
            );
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(
            state["P1"]["deck"],
            if enemy {
                json!(["second"])
            } else {
                json!(["first", "second"])
            }
        );
        if enemy {
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(state["P1"]["cemetery"], json!(["first"]));
        }
    }
}

#[test]
fn other_trait_field_bp03_032_draws_only_with_another_allied_fable_follower() {
    for (zone, trait_name, expected) in [
        ("field", "童話", true),
        ("field", "魔法使い", false),
        ("ex", "童話", false),
    ] {
        let mut initial = position("BP03-032", "P1");
        add_companion(&mut initial, "P1", zone);
        let mut game = companion_game(&initial, "フォロワー", trait_name);
        enter(&mut game, "P1");
        resolved(trigger(&mut game, "BP03-032"));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            usize::from(expected)
        );
        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            if expected { 1_usize } else { 2_usize }
        );
    }
}

#[test]
fn other_trait_field_bp03_052_and_alias_create_a_pawn_and_check_chess_at_own_end() {
    for card in ["BP03-052", "PR-078"] {
        for companion in [false, true] {
            let mut initial = position(card, "P1");
            initial["players"]["P1"]["leader"]["life"] = json!(18_u8);
            if companion {
                add_companion(&mut initial, "P1", "field");
            }
            let mut game = companion_game(&initial, "フォロワー", "チェス");
            enter(&mut game, "P1");
            resolved(trigger(&mut game, card));
            game.resume_internal_card().unwrap();
            let before_end = game.projection(View::Referee).unwrap();
            assert_eq!(before_end["P1"]["ex"].as_array().unwrap().len(), 1_usize);
            let token = before_end["P1"]["ex"][0].as_str().unwrap();
            assert_eq!(before_end["objects"][token]["name"], "マジカルポーン");
            assert_eq!(before_end["objects"][token]["power"], 2_u8);
            assert_eq!(before_end["objects"][token]["hp"], 1_u8);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            resolved(trigger(&mut game, card));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if companion { 19_u8 } else { 18_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if companion { 19_u8 } else { 20_u8 }
            );
        }
    }
}
