#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, cemetery: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] = cemetery;
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(step, CardPlay::Entered(_)), "the follower enters");
    finish_checks(game);
}

/// Opens the printed Fanfare and returns the offered target cards before answering `chosen`.
fn fanfare(game: &mut Game, chosen: &str) -> Vec<String> {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let option = options.iter().find(|o| o.line == 1).unwrap();
    let mut step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap();
    let mut offered = Vec::new();
    while let CardPlay::Awaiting(target) = step {
        let InputDomain::Targets(d) = &target.domain else {
            panic!("unexpected printed decision");
        };
        offered = d.members.iter().filter_map(|m| m.card.clone()).collect();
        let member = d
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(chosen))
            .unwrap();
        step = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![member.handle]),
            )
            .unwrap();
    }
    finish_checks(game);
    offered.sort();
    offered
}

fn kristoff_cemetery(fillers: usize) -> Value {
    let mut cards = vec![
        json!({"id":"revived","card":"BP01-110"}),
        json!({"id":"costly","card":"BP01-104"}),
        json!({"id":"same-name","card":"BP13-077"}),
    ];
    cards.extend((0..fillers).map(|i| json!({"id":format!("royal{i}"),"card":"BP01-037"})));
    Value::Array(cards)
}

#[test]
fn bp13_077_all_printings_revive_an_other_named_cheap_nightmare_with_ward_at_necrocharge_10() {
    for printing in ["BP13-077", "EBD03-009", "PR-485"] {
        let mut game = game(&position(printing, kristoff_cemetery(7)));
        play(&mut game);
        // Same name, cost 6 and non-Nightmare cards are never offered.
        assert_eq!(fanfare(&mut game, "BP01-110"), ["BP01-110"], "{printing}");
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("revived")),
            "{printing}"
        );
        assert!(
            view["objects"]["revived"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "{printing}"
        );
        assert!(
            !view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "{printing}"
        );
    }
}

#[test]
fn bp13_077_below_necrocharge_10_still_chooses_but_leaves_the_card_in_the_cemetery() {
    let mut game = game(&position("BP13-077", kristoff_cemetery(6)));
    play(&mut game);
    assert_eq!(fanfare(&mut game, "BP01-110"), ["BP01-110"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("revived"))
    );
}

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

/// Answers every printed target question, preferring `chosen` and otherwise the first members.
fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) {
    while let CardPlay::Awaiting(q) = step {
        if matches!(q.domain, InputDomain::WardEntry { .. }) {
            step = game
                .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
                .unwrap();
            continue;
        }
        let InputDomain::Targets(d) = &q.domain else {
            panic!("unexpected printed decision: {:?}", q.domain);
        };
        let member = d
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(chosen))
            .or_else(|| d.members.first())
            .unwrap();
        step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![member.handle]))
            .unwrap();
    }
    finish_checks(game);
}

fn trigger(game: &mut Game, line: u64, chosen: &str) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let option = options.iter().find(|o| o.line == line).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap();
    answer(game, step, chosen);
}

fn pass_quick(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    answer(game, step, "");
}

fn banished(count: usize) -> Value {
    (0..count)
        .map(|i| json!({"id":format!("gone{i}"),"card":"BP01-037"}))
        .collect()
}

#[test]
fn bp18_041_all_printings_draw_then_banish_a_hand_card() {
    for printing in ["BP18-041", "BP18-SL11"] {
        let mut initial = position(printing, json!([]));
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"BP01-110"}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game, 1, "BP01-110");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["banish"], json!(["old"]), "{printing}");
        assert_eq!(view["P1"]["hand"], json!(["drawn"]), "{printing}");
    }
}

#[test]
fn bp18_041_lastword_banishes_its_new_cemetery_generation() {
    let mut initial = position("BP01-037", json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-041"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    pass_quick(&mut game);
    trigger(&mut game, 2, "");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!([]));
    assert_eq!(view["P1"]["cemetery"], json!([]));
    assert_eq!(view["P1"]["banish"], json!(["source"]));
}

#[test]
fn bp18_041_activation_needs_ten_banished_cards() {
    let ability = activated("BP18-041");
    for (count, usable) in [(9, false), (10, true)] {
        let mut initial = position("BP01-037", json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-041"}]);
        initial["players"]["P1"]["zones"]["banish"] = banished(count);
        let mut game = game(&initial);
        let step = game.activate_internal_card("source", ability, actor());
        if !usable {
            step.unwrap_err();
            continue;
        }
        answer(&mut game, step.unwrap(), "BP01-037");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(view["objects"]["source"]["acted"], true);
    }
}

fn own_followers() -> Selector {
    let mut selector = subjects();
    if let Selector::Zone { players, .. } = &mut selector {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    Selector::Difference {
        input: Box::new(selector),
        except: Box::new(Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        }),
    }
}

/// Destroys the chosen own follower through an ability, as Lastword requires.
fn destroy(game: &mut Game, chosen: &str) {
    let step = game
        .activate_internal_card("producer", AbilityId(1), actor())
        .unwrap();
    answer(game, step, chosen);
}

fn only_pending(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    assert_eq!(options.len(), 1, "one printed trigger waits");
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    answer(game, step, "");
}

#[test]
fn bp14_076_revives_at_one_one_then_its_lastword_banishes_the_cemetery_card() {
    let mut initial = position("BP01-037", json!([
        {"id":"revived","card":"BP01-104"},
        {"id":"costly","card":"BP01-103"}
    ]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-076"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: own_followers(),
        },
    );
    let step = game
        .activate_internal_card("source", activated("BP14-076"), actor())
        .unwrap();
    let CardPlay::Awaiting(q) = &step else {
        panic!("the revive target is chosen");
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("target question");
    };
    let offered: Vec<_> = d.members.iter().filter_map(|m| m.card.as_deref()).collect();
    assert_eq!(offered, ["BP01-104"], "original cost 9 is not offered");
    answer(&mut game, step, "BP01-104");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["acted"], true);
    assert_eq!(view["objects"]["revived"]["power"], 1_u8);
    assert_eq!(view["objects"]["revived"]["hp"], 1_u8);

    destroy(&mut game, "BP14-076");
    only_pending(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["banish"], json!(["source"]));
    assert!(
        !after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}

#[test]
fn bp12_071_granted_lastword_banishes_the_cemetery_generation() {
    let mut cemetery = vec![json!({"id":"source","card":"BP12-071"})];
    cemetery.extend((0_u8..9).map(|i| json!({"id":format!("royal{i}"),"card":"BP01-037"})));
    let mut initial = position("BP01-037", Value::Array(cemetery));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: own_followers(),
        },
    );
    let step = game
        .activate_internal_card("source", activated("BP12-071"), actor())
        .unwrap();
    answer(&mut game, step, "");
    // Fanfare also triggers on an ability entry; the empty deck mills nothing.
    only_pending(&mut game);
    destroy(&mut game, "BP12-071");
    only_pending(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["banish"], json!(["source"]));
    assert_eq!(view["P1"]["cemetery"].as_array().unwrap().len(), 9);
}
