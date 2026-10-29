#![expect(
    clippy::panic,
    reason = "A different decision invalidates the scenario."
)]

use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected targets")
    };
    question
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("trigger")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}

fn answer_only(game: &mut Game, question: &DecisionView, card: &str) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the matching object is selectable"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(card),
        "the allowed printing matches the filter"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
}

#[test]
fn bp01_139_simple_filter_uses_remaining_life_and_excludes_allies_and_amulets() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-139"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"damaged","card":"BP04-117","state":{"damage":5_u8}},
        {"id":"healthy","card":"BP04-117"},
        {"id":"amulet","card":"BP09-102"}
    ]);
    let mut game = game(&initial);
    let question = awaiting(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    answer_only(&mut game, &question, "BP04-117");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["banish"], json!(["damaged"]));
    assert_eq!(state["P2"]["field"], json!(["healthy", "amulet"]));
    assert_eq!(state["P1"]["field"], json!(["ally"]));
}

#[test]
fn bp06_119_simple_filter_original_cost_includes_amulets_but_rejects_three_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-119"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"cheap","card":"BP09-102"},
        {"id":"expensive","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let question = awaiting(trigger(&mut game, "BP06-119"));
    answer_only(&mut game, &question, "BP09-102");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["expensive"]));
    assert_eq!(state["P2"]["cemetery"], json!(["cheap"]));
}

#[test]
fn bp09_102_simple_filter_lastword_cancels_without_legal_low_life_target() {
    for available in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-102"}]);
        initial["players"]["P2"]["zones"]["field"] = if available {
            json!([{"id":"victim","card":"BP01-037","state":{"damage":2_u8}}])
        } else {
            json!([{"id":"healthy","card":"BP04-117"}])
        };
        let mut objects = subjects();
        if let Selector::Zone { players, .. } = &mut objects {
            players.relation = PlayerRelation::SelfPlayer;
            players.cardinality = Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            };
        }
        let mut game = with_producer(&initial, Action::Destroy { objects });
        activate(&mut game, "P1");
        game.resume_internal_card().unwrap();
        let play = trigger(&mut game, "BP09-102");
        if available {
            let question = awaiting(play);
            answer_only(&mut game, &question, "BP01-037");
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["field"],
                json!([])
            );
        } else {
            assert_eq!(play, CardPlay::Cancelled);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["field"],
                json!(["healthy"])
            );
        }
    }
}

fn synthetic_filter(initial: &Value, from: &Value) -> Game {
    let (mut snapshot, keywords, mut input) = inputs().clone();
    snapshot.push_str(
        &json!({"number":"synthetic-filter","faces":[{
            "name":"Synthetic filter","card_class":"ニュートラル","card_type":"スペル",
            "cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]
        }]})
        .to_string(),
    );
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!(["synthetic-filter.yaml",
        "format: 1\nkind: effect_set\ncards:\n  synthetic-filter:\n    status: complete\n    abilities: []\n"]));
    let mut tree = expanded::spell_tree().clone();
    let mut card = tree["cards"]["SD03-005"].clone();
    card["abilities"][0]["spell"] = json!({
        "targets":[{"bind":"target","from":from,"n":1_u8}],
        "do":[{"destroy":"target"}]
    });
    tree["cards"] = json!({"synthetic-filter":card});
    let program = lowerer()
        .lower_with_printed_costs(
            &tree,
            &BTreeMap::from([("synthetic-filter".into(), Nat(1))]),
        )
        .unwrap()
        .remove("synthetic-filter")
        .unwrap();
    let mut game = game_from_selection(initial, &snapshot, &keywords, &input);
    game.load_internal_card("synthetic-filter", &program)
        .unwrap();
    game
}

#[test]
fn simple_filter_literal_cost_comparisons_intersect_with_kind_and_owner() {
    for (filter, limit, expected) in [
        ("cost_eq", 5_u8, "BP01-037"),
        ("cost_le", 2, "BP01-046"),
        ("cost_ge", 6, "BP04-117"),
        ("orig_cost_eq", 5, "BP01-037"),
        ("orig_cost_le", 2, "BP01-046"),
        ("orig_cost_ge", 6, "BP04-117"),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell","card":"synthetic-filter"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":expected}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"low","card":"BP01-046"},{"id":"mid","card":"BP01-037"},
            {"id":"high","card":"BP04-117"}
        ]);
        let mut from = json!({"who":"opp","zone":"field","kind":"follower"});
        from[filter] = json!(limit);
        let mut game = synthetic_filter(&initial, &from);
        let question = awaiting(
            game.play_internal_card("spell", PlayerId { seat: 1 })
                .unwrap(),
        );
        answer_only(&mut game, &question, expected);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["field"], json!(["ally"]));
    }
}

#[test]
fn simple_filter_effective_and_original_cost_read_different_characteristics() {
    for (filter, expected) in [("cost_eq", "BP04-117"), ("orig_cost_eq", "BP01-037")] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell","card":"synthetic-filter"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"discounted","card":"BP01-037","state":{"cost":1_u8}},
            {"id":"changed","card":"BP04-117","state":{"cost":5_u8}}
        ]);
        let mut from = json!({"who":"opp","zone":"field","kind":"follower"});
        from[filter] = json!(5_u8);
        let mut game = synthetic_filter(&initial, &from);
        let question = awaiting(
            game.play_internal_card("spell", PlayerId { seat: 1 })
                .unwrap(),
        );
        answer_only(&mut game, &question, expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["field"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
