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
    let mut initial = position(
        "BP01-037",
        json!([
            {"id":"revived","card":"BP01-104"},
            {"id":"costly","card":"BP01-103"}
        ]),
    );
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
