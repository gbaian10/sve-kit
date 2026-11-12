#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the paid damage scenarios."
)]
use super::event_paid_consumer::{lower, position, question, state};
use super::*;
use sve_engine::game::internal::InputDomain;

fn observer(reset: bool) -> WellFormedCardProgram {
    let mut steps = vec![json!({"damage":{"to":"it","n":1}})];
    if reset {
        steps.push(json!({"stand":"this"}));
    }
    lower(&json!([{"pay":{"cost":{"act_self":true},"do":steps}}]))
}

fn aura_follower(mut abilities: Vec<Located<Ability>>) -> WellFormedCardProgram {
    let mut aura = static_keywords::conditional_keyword();
    aura.node.id = AbilityId(500);
    aura.node.scope = ScopeId(500);
    let Effect::Continuous {
        condition, patch, ..
    } = &mut aura.node.declarations[0]
    else {
        panic!("static keyword")
    };
    *condition = None;
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Keyword {
            keyword: KeywordId::Aura,
        }],
    };
    abilities.push(aura);
    card(abilities)
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pending(game: &mut Game, number: &str) -> CardPlay {
    finish_checks(game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("entry occurrence")
    };
    let chosen = options.iter().find(|option| option.card == number).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(chosen.handle))
        .unwrap()
}

fn enter(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("incoming", PlayerId { seat: 2 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the opposing follower enters before its observer resolves"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["incoming"]["keywords"],
        json!(["オーラ"]),
        "the opposing event subject really carries Aura"
    );
}

#[test]
fn paid_damage_reaches_an_opposing_aura_subject_only_after_actual_payment() {
    let program = observer(false);
    let aura = aura_follower(vec![]);
    for (accept, acted, paid) in [
        (true, false, true),
        (false, false, false),
        (true, true, false),
    ] {
        let mut game = position(&program, &state(acted), &aura, &card(vec![]));
        enter(&mut game);
        let cost = question(pending(&mut game, "unit-amulet"));
        let result = resolved(
            game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(accept))
                .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["incoming"]["damage"], u8::from(paid));
        assert_eq!(view["objects"]["observer"]["acted"], acted || paid);
        assert_eq!(view["P1"]["leader"]["life"], 20);
        assert_eq!(view["P2"]["leader"]["life"], 20);
        assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(paid) && (paid || payment.steps.is_empty()))), "only the completed payment enables event-subject damage");
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Damage(_))),
            paid
        );
    }
}

#[test]
fn an_earlier_entry_ability_removes_the_source_before_its_damage_payment() {
    let entering = lowerer()
        .lower(&equipment_played::tree(
            &json!({"fanfare":{"do":[
                {"destroy":{"ref":{"who":"opp","zone":"field","kind":"amulet"}}}
            ]}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let follower = aura_follower(entering.card().abilities.clone());
    let mut game = position(&observer(false), &state(false), &follower, &card(vec![]));
    enter(&mut game);
    let destruction = resolved(pending(&mut game, "unit-follower"));
    assert!(
        destruction.progress.facts.iter().any(|fact| matches!(fact,
            Fact::Movement(movement) if movement.method == MoveMethod::Destroy
                && movement.card_before == "unit-amulet")),
        "the earlier pending actually destroys the payment source"
    );
    let cost = question(pending(&mut game, "unit-amulet"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["observer"])
    );
    let result = resolved(
        game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["incoming"]["damage"], 0);
    assert_eq!(view["P2"]["field"], json!(["incoming"]));
    assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(false) && payment.steps.is_empty())), "a previously triggered ability cannot pay with its departed source");
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(_))),
        "failed payment never executes the damage continuation"
    );
}

fn repeated_entry() -> WellFormedCardProgram {
    let movement = |id, from, destination| {
        let binding = Binding::new(100_u64.checked_add(id).unwrap(), ScopeId(1));
        instruction(
            id,
            Action::Move {
                objects: Selector::Zone {
                    players: players(PlayerRelation::SelfPlayer),
                    zone: from,
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
                        left: Expr::Live(Box::new(KindRead::Kind {
                            object: ObjectView::Current {
                                object: Box::new(ObjectOperand::Bound { binding }),
                            },
                            view: CharacteristicView::Effective,
                        })),
                        right: Expr::Literal(CardKind::Follower),
                    }))),
                },
                movement: MoveSpec {
                    to: destination,
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
        )
    };
    let entry = MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Field,
        },
    };
    card(vec![play::spell(vec![
        movement(1, ZoneKind::Cemetery, entry.clone()),
        movement(
            2,
            ZoneKind::Field,
            MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
        ),
        movement(3, ZoneKind::Cemetery, entry),
    ])])
}

#[test]
fn successful_payments_do_not_retarget_an_old_subject_to_a_new_generation() {
    let mut initial = state(false);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"producer","card":"unit-spell"}]);
    initial["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"incoming","card":"unit-follower"}]);
    let mut game = position(
        &observer(true),
        &initial,
        &aura_follower(vec![]),
        &repeated_entry(),
    );
    resolved(
        game.play_internal_card("producer", PlayerId { seat: 2 })
            .unwrap(),
    );
    finish_checks(&mut game);
    let mut count = 0_u8;
    while game.internal_pending_choice().unwrap().is_some() {
        let cost = question(pending(&mut game, "unit-amulet"));
        let result = resolved(
            game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(true))), "each occurrence really pays, including the old subject generation");
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["observer"]["acted"],
            false
        );
        finish_checks(&mut game);
        count = count.checked_add(1).unwrap();
        assert!(
            count <= 2,
            "only the two sequential entries have an occurrence"
        );
    }
    assert_eq!(count, 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["incoming"]["damage"],
        1
    );
}

#[test]
fn a_paid_damage_recipient_cannot_use_a_foreign_event() {
    let mut raw = observer(false).card().clone();
    let Node::Control {
        control: Control::Pay { on_paid, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("payment")
    };
    let Node::Instruction { instruction } = &mut on_paid.steps[0].node else {
        panic!("damage")
    };
    let Action::Damage {
        recipients:
            RecipientSet::Objects {
                objects: Selector::Objects { objects },
            },
        ..
    } = &mut instruction.action
    else {
        panic!("event recipient")
    };
    objects[0] = ObjectOperand::FromExpression {
        value: Box::new(Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject {
            event: Binding::new(999, ScopeId(999)),
        }))),
    };
    assert_eq!(
        WellFormedCardProgram::check(raw).unwrap_err().reason,
        WellFormedReason::UnsupportedShape
    );
}
