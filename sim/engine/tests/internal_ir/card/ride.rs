use super::*;
use sve_engine::game::internal::InputDomain;

fn gain(id: u64) -> Located<Node> {
    instruction(
        id,
        Action::RideDrive {
            subject: ObjectOperand::SourceHolder {},
            operation: RideDriveOperation::GainDrive {},
            usage: UsageReference::CurrentRulePermission,
        },
    )
}
fn gained(result: &Activation) -> usize {
    result
        .progress
        .facts
        .iter()
        .filter(|f| matches!(f, Fact::DriveGained(_)))
        .count()
}
#[test]
fn gain_drive_is_once_per_generation_even_across_repeated_ability_executions() {
    for seat in [1, 2] {
        let mut game = position(seat, vec![gain(1), gain(2)]);
        assert_eq!(gained(&activate(&mut game, seat)), 1);
        game.resume_internal_card().unwrap();
        assert_eq!(gained(&activate(&mut game, seat)), 0);
    }
}
#[test]
fn leaving_and_returning_creates_a_new_generation_that_can_gain_drive_again() {
    let bounce = movement::put(
        2,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Hand,
    );
    let program = card(vec![ability(1, vec![gain(1)]), ability(2, vec![bounce])]);
    let mut game = position_with_card(1, &program);
    assert_eq!(gained(&activate(&mut game, 1)), 1);
    game.resume_internal_card().unwrap();
    resolved(
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let play = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    assert!(matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)));
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert_eq!(gained(&activate(&mut game, 1)), 1);
}

#[test]
fn ride_payment_cannot_inspect_an_opponents_evolve_deck_or_choose_a_different_named_resource() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["ride"] = json!({"cost":{"pp":1}});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    let programs = lowerer().lower(&tree).unwrap();
    for foreign in [false, true] {
        let mut candidate = programs["unit-follower"].card().clone();
        let parameters = &mut candidate.abilities[0]
            .node
            .play
            .as_mut()
            .unwrap()
            .parameters;
        let ChoiceDomain::Objects { constraint, .. } = &mut parameters[0].domain else {
            panic!("fixed Ride resource selection")
        };
        let Selector::Zone {
            players, predicate, ..
        } = &mut constraint.groups[0].selector
        else {
            panic!("fixed Ride resource selection")
        };
        if foreign {
            players.relation = PlayerRelation::Opponents;
        } else {
            **predicate = Expr::Literal(Bool(true));
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

#[test]
fn acquired_single_drive_subscribes_even_without_any_printed_attack_abilities() {
    let mut game = position(1, vec![gain(1)]);
    activate(&mut game, 1);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("acquired Single Drive pending")
    };
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|f| matches!(f, Fact::DriveChecked(_)))
            .count(),
        1
    );
}

#[test]
fn ordinary_grants_cannot_bypass_drive_acquisition_or_expand_single_drive() {
    for keyword in [KeywordId::Drive, KeywordId::SingleDrive] {
        let grant = instruction(
            1,
            Action::Grant {
                subjects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                abilities: vec![GrantedAbility::Keyword { keyword }],
                duration: Duration {
                    end: DurationEnd::NoTemporalExpiry {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
                provider: TextProvider {
                    provider: ObjectView::Current {
                        object: Box::new(ObjectOperand::SourceHolder {}),
                    },
                    face: FaceId("0".into()),
                    ability: AbilityId(1),
                },
            },
        );
        WellFormedCardProgram::check(CardProgram {
            abilities: vec![ability(1, vec![grant])],
            card_play_costs: vec![],
        })
        .unwrap_err();
    }
}

#[test]
fn a_departed_holder_cannot_acquire_drive_from_the_rest_of_its_body() {
    let bounce = movement::put(
        1,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Hand,
    );
    let mut game = position(1, vec![bounce, gain(2)]);
    assert_eq!(gained(&activate(&mut game, 1)), 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        json!([])
    );
}

#[test]
fn gain_drive_ignores_a_non_follower_holder() {
    let mut entry = ability(1, vec![gain(1)]);
    entry.source.card_id = "unit-amulet".into();
    entry.source.card_no = "unit-amulet".into();
    locate_steps(
        &mut entry.node.body.as_mut().unwrap().steps,
        &entry.source,
        &entry.node.source,
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "amulet-drive",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![entry]))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    assert_eq!(gained(&activate(&mut game, 1)), 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        json!([])
    );
}
