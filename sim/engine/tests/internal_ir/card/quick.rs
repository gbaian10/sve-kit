#![expect(
    clippy::panic,
    reason = "Unexpected fixture shapes fail the test immediately."
)]

use super::*;
use sve_engine::game::Step;
use sve_engine::game::internal::{Answer, DecisionView, InputDomain};

pub(super) fn permit() -> Located<Ability> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    let mut header = seed["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("activate");
    header["line"] = json!(2);
    header["keywords"] = json!(["quick"]);
    header["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("ability_scope");
    seed["abilities"] = json!([header]);
    tree["cards"] = json!({"unit-spell": seed});
    let lowered = lowerer().lower(&tree).unwrap();
    let mut a = lowered["unit-spell"].card().abilities[0].clone();
    a.node.id = AbilityId(2);
    a.node.scope = ScopeId(2);
    a
}

fn quick_activated(mut a: Located<Ability>) -> Located<Ability> {
    a.node.source.origin = AbilityOrigin::Keyword {
        keyword: KeywordId::Quick,
        origin: Box::new(a.node.source.origin.clone()),
    };
    if let CostSpec::Ordered { steps } = &mut a.node.play.as_mut().unwrap().costs {
        for step in steps {
            step.instruction.source = a.node.source.clone();
        }
    }
    locate_steps(
        &mut a.node.body.as_mut().unwrap().steps,
        &a.source,
        &a.node.source,
    );
    a
}

fn board(spell: &WellFormedCardProgram, follower: &WellFormedCardProgram, defender: bool) -> Game {
    let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"a","card":"unit-follower","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["pp"] = json!({"current":2,"max":2});
    initial["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"q1","card":"unit-spell"},{"id":"q2","card":"unit-spell"}]);
    initial["players"]["P2"]["zones"]["field"] = if defender {
        json!([{"id":"b","card":"unit-follower"}])
    } else {
        json!([])
    };
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "quick-seed",
    )
    .unwrap();
    game.load_internal_card("unit-spell", spell).unwrap();
    game.load_internal_card("unit-follower", follower).unwrap();
    game
}

fn question(game: &mut Game) -> DecisionView {
    game.internal_quick_choice().unwrap().unwrap()
}

fn choose(game: &mut Game, q: &DecisionView, handle: Option<u64>) -> CardPlay {
    game.answer_internal_card(q.actor, q.token, Answer::Quick(handle))
        .unwrap()
}

pub(super) fn pass(game: &mut Game) -> Step {
    let q = question(game);
    let CardPlay::Continued(step) = choose(game, &q, None) else {
        panic!("expected Quick Pass");
    };
    step
}

fn pending(game: &mut Game) -> PlayerId {
    let q = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(1))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    q.actor
}

#[test]
fn attack_quick_repeats_after_pending_even_without_the_attacker() {
    let spell = card(vec![
        play::spell(vec![instruction(
            1,
            Action::Destroy {
                objects: zone(ZoneKind::Field, 9),
            },
        )]),
        permit(),
    ]);
    let follower = card(vec![
        timing::on_attack(ability(1, vec![draw(1, 0)])),
        triggers::lastword(ability(
            2,
            vec![entry::enter(2, entry::own_zone(ZoneKind::Cemetery, 22))],
        )),
    ]);
    // Without a defender, P1's cemetery holds only the attacker that each Lastword returns.
    let mut game = board(&spell, &follower, false);
    game.internal_rule_action(&json!({"do":"attack","attacker":"a","target":"P2.leader"}))
        .unwrap();
    game.internal_quick_choice().unwrap_err();
    assert_eq!(pending(&mut game), PlayerId { seat: 1 });
    let q = question(&mut game);
    assert_eq!(q.actor, PlayerId { seat: 2 });
    let before = exact_authority(&game);
    game.play_internal_card("q1", q.actor).unwrap_err();
    game.submit(&json!({"do":"pass"})).unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let first = resolved(choose(&mut game, &q, Some(0)));
    assert!(matches!(
        first.progress.facts.first(),
        Some(Fact::CardPlayed { .. })
    ));
    game.internal_quick_choice().unwrap_err();
    game.resume_internal_card().unwrap();
    assert_eq!(pending(&mut game), PlayerId { seat: 1 });
    let next = question(&mut game);
    assert!(next.token.epoch > q.token.epoch);
    resolved(choose(&mut game, &next, Some(0)));
    game.resume_internal_card().unwrap();
    assert_eq!(pending(&mut game), PlayerId { seat: 1 });
    let last = question(&mut game);
    assert!(matches!(&last.domain, InputDomain::Quick { options, .. } if options.is_empty()));
    pass(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["pp"]["current"], 0);
    assert_eq!(view["P2"]["cemetery"], json!(["q1", "q2"]));
    assert_eq!(view["P1"]["field"], json!(["a"]));
    assert_eq!(view["P2"]["leader"]["life"], 20);
    assert!(game.internal_quick_choice().unwrap().is_none());
}

#[test]
fn quick_activation_pays_act_without_granting_card_play_and_main_spell_stays_ordinary() {
    let follower = card(vec![quick_activated(payment::paid_ability(
        vec![payment::act()],
        vec![draw(1, 0)],
    ))]);
    let spell = card(vec![play::spell(vec![draw(1, 0)]), permit()]);
    let mut main = play::position(&spell, "hand", 0);
    resolved(main.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    main.resume_internal_card().unwrap();
    assert_eq!(
        main.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3
    );
    let mut game = board(&card(vec![]), &follower, true);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = question(&mut game);
    assert!(
        matches!(&q.domain, InputDomain::Quick { options, .. } if options.len() == 1 && options[0].line == Some(1))
    );
    let before = exact_authority(&game);
    game.activate_internal_card("a", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let result = resolved(choose(&mut game, &q, Some(0)));
    assert!(matches!(
        result.progress.facts.first(),
        Some(Fact::Posture(_))
    ));
    game.resume_internal_card().unwrap();
    assert!(
        matches!(question(&mut game).domain, InputDomain::Quick { options, .. } if options.is_empty())
    );
    pass(&mut game);
    // A Quick Activated header has no card-play permission in Hand.
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"h","card":"unit-follower"}]);
    let mut hand = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "hand-quick",
    )
    .unwrap();
    hand.load_internal_card("unit-follower", &follower).unwrap();
    hand.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    hand.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert!(
        matches!(question(&mut hand).domain, InputDomain::Quick { options, .. } if options.is_empty())
    );
}

#[test]
fn quick_protocol_rejects_and_failed_play_restores_only_this_origin_with_fresh_epoch() {
    let failure = Expr::Compute(Box::new(IntOperation::Conditional {
        condition: Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::PpCurrent,
            })),
            comparison: Comparison::Equal,
            right: Expr::Literal(Int(0)),
        })),
        then_value: Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Add,
            arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
        })),
        else_value: Expr::Literal(Int(0)),
    }));
    let mut ability = play::spell(vec![draw_value(1, failure)]);
    ability
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(play::target(5, 1, 0, 1));
    let mut game = board(&card(vec![ability, permit()]), &card(vec![]), true);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = question(&mut game);
    let before = exact_authority(&game);
    game.answer_internal_card(PlayerId { seat: 1 }, q.token, Answer::Quick(None))
        .unwrap_err();
    game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(0)))
        .unwrap_err();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(99)))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let private = game.projection(View::P2).unwrap();
    assert_eq!(private["awaiting"]["domain"]["options"][0]["zone"], "hand");
    assert_eq!(private["awaiting"]["domain"]["options"][0]["position"], 1);
    assert_eq!(private["awaiting"]["domain"]["options"][1]["position"], 2);
    assert_eq!(
        private["awaiting"]["domain"]["options"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(game.projection(View::P1).unwrap()["awaiting"]["domain"].is_null());
    let CardPlay::Awaiting(target) = choose(&mut game, &q, Some(0)) else {
        panic!("targets");
    };
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let q2 = question(&mut game);
    let before = exact_authority(&game);
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let CardPlay::Awaiting(target) = choose(&mut game, &q2, Some(0)) else {
        panic!("targets");
    };
    game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![]))
        .unwrap_err();
    let retry = question(&mut game);
    assert!(retry.token.epoch > target.token.epoch);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["pp"]["current"], 1);
    assert_eq!(view["P2"]["cemetery"], json!(["q1"]));
    assert_eq!(view["P2"]["hand"], json!(["q2"]));
    game.concede_internal(PlayerId { seat: 1 }).unwrap();
    assert!(game.internal_quick_choice().unwrap().is_none());
    assert!(game.internal_decision(View::Referee).is_none());
    let ended = exact_authority(&game);
    game.answer_internal_card(retry.actor, retry.token, Answer::Quick(None))
        .unwrap_err();
    assert_eq!(ended, exact_authority(&game));
}

#[test]
fn quick_admission_keeps_exact_permission_and_rejects_a_quick_spell_body_origin() {
    let mut broad = permit();
    let Effect::Permission { subject, .. } = &mut broad.node.declarations[0] else {
        panic!("permission");
    };
    *subject = PermissionSubject::Objects {
        objects: zone(ZoneKind::Hand, 1),
    };
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![play::spell(vec![]), broad],
        card_play_costs: vec![],
    })
    .unwrap_err();
    let mut bad = play::spell(vec![]);
    bad.node.source.origin = AbilityOrigin::Keyword {
        keyword: KeywordId::Quick,
        origin: Box::new(bad.node.source.origin.clone()),
    };
    if let CostSpec::Ordered { steps } = &mut bad.node.play.as_mut().unwrap().costs {
        for step in steps {
            step.instruction.source = bad.node.source.clone();
        }
    }
    let mut game = game(1, 1);
    let before = exact_authority(&game);
    game.load_internal_card("unit-spell", &card(vec![bad]))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
}
