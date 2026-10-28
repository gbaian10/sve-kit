use super::*;

use super::rule_fixtures;
use sve_engine::catalog::EngineIdentityInput;

fn banish(id: u64, objects: Selector) -> Located<Node> {
    instruction(id, Action::Banish { objects })
}

fn posture(id: u64, mut objects: Selector, posture: Posture) -> Located<Node> {
    if let Selector::Zone { binding, .. } = &mut objects {
        *binding = Binding::new(id, ScopeId(1));
    }
    instruction(id, Action::Posture { objects, posture })
}

fn position(seat: u32) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["players"][actor]["zones"]["ex"] = json!([{"id":"material","card":"unit-follower"}]);
    let snapshot = [("unit-spell", "スペル"), ("unit-follower", "フォロワー"), ("lesson", "スペル・トークン")].iter().map(|(number, kind)| {
        json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()
    }).collect::<Vec<_>>().join("\n");
    let document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","abilities":[]},
        "unit-follower":{"status":"complete","abilities":[]},
        "lesson":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([]),
        json!([rule_fixtures::resource(
            "lesson_item",
            "lesson",
            "printed",
            true
        )]),
    )
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents_with_rules(
            &snapshot,
            "format: 1\nkind: keyword_registry\nkeywords: {}\n",
            &[("synthetic".into(), document)],
            &rules,
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    );
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "basic-actions",
    )
    .unwrap()
}

#[test]
fn bp01_013_public_ex_banish_fragment_has_a_real_method_success_and_new_generation() {
    for seat in [1, 2] {
        let mut game = position(seat);
        let result = game
            .execute_internal(
                &checked(vec![banish(1, zone(ZoneKind::Ex, 1))]),
                &source("BP01-013"),
                context(seat),
            )
            .unwrap();
        let Some(InstructionOutcome::Banish(outcome)) = result.receipts.get(&ReceiptId(1)) else {
            panic!("banish outcome");
        };
        assert_eq!(outcome.successful.len(), 1);
        assert_eq!(outcome.moves.len(), 1);
        let movement = &outcome.moves[0];
        assert_eq!(movement.before.generation, Generation(0));
        assert_eq!(movement.after.unwrap().generation, Generation(1));
        assert_eq!(movement.actual_to.unwrap().kind, ZoneKind::Banish);
        assert!(
            matches!(result.facts.as_slice(), [Fact::Movement(fact)] if fact.method == MoveMethod::Banish && fact.from.kind == ZoneKind::Ex)
        );
        let actor = if seat == 1 { "P1" } else { "P2" };
        assert_eq!(
            game.projection(View::Referee).unwrap()[actor]["banish"],
            json!(["material"])
        );
    }
}

#[test]
fn put_to_banish_cannot_produce_a_banish_receipt_or_method() {
    let mut game = position(1);
    let put = movement::put(1, zone(ZoneKind::Ex, 1), OwnerZone::Banish);
    let result = game
        .execute_internal(
            &checked(vec![put.clone()]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(matches!(
        result.receipts.get(&ReceiptId(1)),
        Some(InstructionOutcome::Move(_))
    ));
    assert!(
        matches!(result.facts.as_slice(), [Fact::Movement(fact)] if fact.method == MoveMethod::Put)
    );
    let fake_banish = banish(
        2,
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Movement {
                receipt: MovementReceipt::Banish {
                    receipt: ReceiptReference::new(ReceiptId(1)),
                },
            },
            projection: MemberProjection::Before,
        },
    );
    WellFormedProgram::check(body(1, vec![put, fake_banish])).unwrap_err();
}

#[test]
fn synthetic_banish_success_projection_follows_only_the_moved_generation() {
    let mut game = position(1);
    let receipt = MovementReceipt::Banish {
        receipt: ReceiptReference::new(ReceiptId(1)),
    };
    let moved = Selector::MoveAfter {
        receipt,
        permission: FollowPermission::SameEffectExplicitReference {},
    };
    let result = game
        .execute_internal(
            &checked(vec![banish(1, zone(ZoneKind::Ex, 1)), banish(2, moved)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    let Some(InstructionOutcome::Banish(second)) = result.receipts.get(&ReceiptId(2)) else {
        panic!("banish outcome");
    };
    assert_eq!(second.successful, vec![]);
    assert_eq!(second.moves.len(), 1);
    assert_eq!(second.moves[0].before.generation, Generation(1));
    assert_eq!(second.moves[0].after, None);
    assert_eq!(result.facts.len(), 1);
}

#[test]
fn cp03_017_posture_fragments_change_each_subject_once_and_ignore_repeated_noops() {
    for seat in [1, 2] {
        let mut game = position(seat);
        let targets = zone(ZoneKind::Field, 1);
        let result = game
            .execute_internal(
                &checked(vec![
                    posture(1, targets.clone(), Posture::Acted),
                    posture(2, targets.clone(), Posture::Acted),
                    posture(3, targets.clone(), Posture::Standing),
                    posture(4, targets, Posture::Standing),
                ]),
                &source("CP03-017"),
                context(seat),
            )
            .unwrap();
        assert!(
            matches!(result.facts.as_slice(), [Fact::Posture(first), Fact::Posture(second)]
            if first.object == second.object && first.before == Posture::Standing && first.after == Posture::Acted && second.before == Posture::Acted && second.after == Posture::Standing)
        );
        for id in [2, 4] {
            assert_eq!(
                result.receipts[&ReceiptId(id)],
                InstructionOutcome::Posture(vec![])
            );
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["f"]["acted"], false);
        assert_eq!(state["objects"]["f"]["generation"], 0);
    }
}

#[test]
fn basic_actions_reject_non_public_origins_and_preserve_an_earlier_posture_change() {
    for action in [
        banish(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        ),
        posture(2, zone(ZoneKind::Ex, 1), Posture::Acted),
    ] {
        let mut game = position(1);
        let before = exact_authority(&game);
        game.execute_internal(
            &checked(vec![
                posture(1, zone(ZoneKind::Field, 1), Posture::Acted),
                action,
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn basic_actions_reject_a_stale_source_before_any_commit() {
    for action in [
        banish(1, zone(ZoneKind::Ex, 1)),
        posture(1, zone(ZoneKind::Field, 1), Posture::Acted),
    ] {
        let mut game = position(1);
        let mut stale = context(1);
        stale.generation = Generation(1);
        let before = exact_authority(&game);
        game.execute_internal(&checked(vec![action]), &source("synthetic"), stale)
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn synthetic_stale_target_does_not_banish_or_change_posture_in_its_new_zone() {
    let mut game = position(1);
    let capture = CaptureRef {
        binding: Binding::new(50, ScopeId(1)),
    };
    let targets = Selector::Captured {
        reference: capture.clone(),
    };
    let result = game
        .execute_internal(
            &checked(vec![
                node(Node::Capture {
                    capture: Capture::Objects {
                        at: CaptureTime::AtResolution { scope: ScopeId(1) },
                        value: zone(ZoneKind::Field, 1),
                        into: capture,
                    },
                }),
                instruction(
                    1,
                    Action::Destroy {
                        objects: targets.clone(),
                    },
                ),
                banish(2, targets.clone()),
                posture(3, targets, Posture::Acted),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert_eq!(
        result.receipts[&ReceiptId(3)],
        InstructionOutcome::Posture(vec![])
    );
    let Some(InstructionOutcome::Banish(outcome)) = result.receipts.get(&ReceiptId(2)) else {
        panic!("banish outcome");
    };
    assert_eq!(outcome.successful, vec![]);
    assert_eq!(outcome.moves[0].after, None);
    assert_eq!(result.facts.len(), 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["f"])
    );
}
