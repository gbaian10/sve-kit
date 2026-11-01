#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate these printed arithmetic scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;
const PLAYER: PlayerId = PlayerId { seat: 1 };

fn printings(number: &str) -> Vec<String> {
    let card_id = &selection().current[number].card_id;
    selection()
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == card_id)
        .map(|(printing, _)| printing.clone())
        .collect()
}
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opponent-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":15_u8,"max_hp":15_u8,"power":0_u8,"acted":true}}]);
    initial
}
fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "number-count-arithmetic",
    )
    .unwrap()
}
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("BP01-037"))
                    .unwrap()
                    .handle,
            ]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected arithmetic decision"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    resolved(play);
}
fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed evolution trigger")
    };
    assert_eq!(options.len(), 1, "exactly one evolution trigger is pending");
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, play);
    game.resume_internal_card().unwrap();
}

#[test]
fn blitz_adds_two_even_with_no_pawns_and_ignores_opposing_and_ex_pawns() {
    for printing in printings("BP03-053") {
        for count in [0_u8, 2, 4] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            let mut field: Vec<Value> = (0..count)
                .map(|i| json!({"id":format!("pawn{i}"),"card":"BP03-T03"}))
                .collect();
            field.push(json!({"id":"ordinary","card":"BP01-037"}));
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP03-T03"}]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"opposing","card":"BP03-T03"}));
            let mut game = new_game(&initial);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                count + 2,
                "printed X is allied field pawn count plus two, including Q480 zero case"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 9_u8,
                "the real spell pays its printed PP cost"
            );
        }
    }
}

#[test]
fn magical_rat_evolves_from_its_full_base_and_deals_twice_the_allied_field_count() {
    for printing in printings("BP06-050") {
        for count in [1_u8, 2, 3] {
            let mut initial = position();
            let mut field = vec![
                json!({"id":"source","card":"BP06-049"}),
                json!({"id":"ordinary","card":"BP01-037"}),
            ];
            field.extend((1..count).map(|i| json!({"id":format!("rat{i}"),"card":"BP06-049"})));
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":printing}]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP06-049"}]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"opposing","card":"BP06-049"}));
            let mut game = new_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the complete basic rat provides its printed evolution ability"
            );
            finish_checks(&mut game);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                count * 2,
                "the evolved source counts itself and deals a single doubled amount"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 9_u8,
                "evolution pays its printed PP1"
            );
        }
    }
}

fn enter(game: &mut Game) {
    let mut result = game.play_internal_card("source", PLAYER).unwrap();
    while let CardPlay::Awaiting(q) = result {
        let InputDomain::WardEntry { .. } = q.domain else {
            panic!("printed ward entry")
        };
        result = game
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(result, CardPlay::Entered(_)),
        "the real follower enters from hand"
    );
    finish_checks(game);
}
fn ability(number: &str, zone: ZoneKind) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated && ability.node.active_zones == [zone]
        })
        .unwrap()
        .node
        .id
}

#[test]
fn commander_counts_three_distinct_allied_soldier_tokens_and_can_discard_itself_from_hand() {
    for printing in printings("BP17-023") {
        for (third, pp) in [("BP17-T13", 8_u8), ("BP01-T05", 2)] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"knight","card":"BP17-T11"},{"id":"steel","card":"BP17-T12"},{"id":"third","card":third}]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP17-T13"}]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"opposing","card":"BP17-T13"}));
            let mut game = new_game(&initial);
            enter(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"], pp,
                "only three distinct allied field soldier tokens reduce PP8 by six"
            );
            assert_eq!(
                state["objects"]["source"]["keywords"],
                json!(["守護"]),
                "the complete commander retains ward"
            );
        }
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        let mut game = new_game(&initial);
        let mut result = game
            .activate_internal_card("source", ability(&printing, ZoneKind::Hand), PLAYER)
            .unwrap();
        while let CardPlay::Awaiting(q) = result {
            let InputDomain::Modes { modes, .. } = &q.domain else {
                panic!("printed three-token alternative")
            };
            result = game
                .answer_internal_card(q.actor, q.token, Answer::Modes(vec![modes[0]]))
                .unwrap();
        }
        resolved(result);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "hand activation pays PP1"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["source"]),
            "the exact activated hand copy is discarded as cost"
        );
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 1, "one selected token is created");
        let id = field[0].as_str().unwrap();
        assert_eq!(
            state["objects"][id]["name"], "スティールナイト",
            "the first printed alternative creates a real Steel Knight"
        );
    }
}

#[test]
fn mistress_hurricane_checks_own_banish_count_then_pays_act_and_self_banish_for_damage() {
    for printing in printings("CP03-048") {
        for count in [4_u8, 5] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["banish"] = json!((0..count)
                .map(|i| json!({"id":format!("banished{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>());
            initial["players"]["P2"]["zones"]["banish"] = json!((0..5_u8)
                .map(|i| json!({"id":format!("opponent-banished{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>());
            let mut game = new_game(&initial);
            enter(&mut game);
            trigger(&mut game);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                count == 5,
                "only five cards in the controller's banish zone grant storm"
            );
            assert_eq!(
                before["P1"]["pp"]["current"], 8_u8,
                "the follower pays its printed PP2"
            );
            if count == 5 {
                let result = game
                    .activate_internal_card("source", ability(&printing, ZoneKind::Field), PLAYER)
                    .unwrap();
                finish(&mut game, result);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["objects"]["enemy"]["damage"], 3_u8,
                    "the paid printed ability deals three damage"
                );
                assert!(
                    state["P1"]["field"].as_array().unwrap().is_empty(),
                    "the source leaves field as a paid cost"
                );
                assert!(
                    state["P1"]["banish"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("source")),
                    "the same physical source is banished as cost"
                );
                assert_eq!(
                    state["P1"]["pp"]["current"], 8_u8,
                    "activation has no extra PP cost"
                );
            }
        }
    }
}
