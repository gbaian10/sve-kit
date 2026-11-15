use super::battle_pending::fixture;
use super::*;
use crate::battle_costs::object;
use sve_battle_contract::observation as wire;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::{Viewer, observation as native};

const fn limit(consume_at: ConsumptionPoint) -> LimitSpec {
    LimitSpec {
        maximum: Nat(1),
        window: LimitWindow::Turn,
        scope: LimitScope::AbilityInstance,
        consume_at,
    }
}

fn board(actor: Viewer, secret: &str, program: &WellFormedCardProgram) -> Game {
    let mut initial = setup(actor.player().seat, json!([]));
    let (own, opponent) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower","state":{"entered_this_turn":false}}
    ]);
    initial["players"][opponent]["zones"]["hand"] = json!([{"id":"secret","card":secret}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-draw1"),"card":"unit-spell"},
            {"id":format!("{seat}-draw2"),"card":"unit-spell"}
        ]);
    }
    let mut game = Game::new(
        Arc::clone(&fixture().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-quota",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn quotas(game: &Game, viewer: Viewer, actor: Viewer) -> Vec<wire::AbilityQuota> {
    let before = exact_authority(game);
    let observation = game.observe(viewer).unwrap();
    let native_handle = object(&observation, actor, native::ObservedZone::Field);
    let handle = sve_battle_contract::ObjectHandle::new(native_handle.as_str().into());
    let table = game.battle_objects(viewer, &fixture().references).unwrap();
    let result = table[&handle]
        .state
        .as_ref()
        .unwrap()
        .quotas
        .clone()
        .unwrap();
    for quota in &result {
        let source = serde_json::to_value(&quota.source).unwrap();
        assert_eq!(source["kind"], "card", "the quota has a printed origin");
        assert_eq!(
            source["printing_id"], "printing-follower",
            "the origin uses the bound permanent printing"
        );
        assert_eq!(
            source["face"], 1,
            "the native face maps to the adopted face"
        );
        assert_eq!(
            source["holder"],
            handle.as_str(),
            "the source links to the current holder"
        );
        assert!(
            table.contains_key(&handle),
            "the source belongs to the same object table"
        );
    }
    assert_eq!(
        exact_authority(game),
        before,
        "reading the quota must not allocate authority state"
    );
    result
}

fn assert_remaining(
    game: &Game,
    actor: Viewer,
    expected: u64,
    kind: wire::AbilityKind,
    consume_at: wire::ConsumptionPoint,
) {
    for viewer in [Viewer::P1, Viewer::P2] {
        let values = quotas(game, viewer, actor);
        assert_eq!(values.len(), 1, "the holder has one limited ability");
        assert_eq!(
            values[0].maximum.get(),
            1,
            "the printed limit remains unchanged"
        );
        assert_eq!(
            values[0].remaining.get(),
            expected,
            "the budget follows the authoritative lifecycle"
        );
        assert_eq!(values[0].ability, kind, "the ability category is preserved");
        assert_eq!(
            values[0].consume_at, consume_at,
            "the consumption stage is preserved"
        );
    }
}

#[test]
fn wire_quota_lifecycle_consumes_activation_only_after_valid_declaration_and_resets_next_turn() {
    let mut activation = ability(1, vec![draw(1, 0)]);
    let play = activation.node.play.as_mut().unwrap();
    play.limit = Some(limit(ConsumptionPoint::PlayCommitted));
    play.parameters.push(play::target(10, 1, 1, 1));
    let program = card(vec![activation]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut games = [
            board(actor, "unit-spell", &program),
            board(actor, "unit-amulet", &program),
        ];
        for game in &mut games {
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("s", AbilityId(1), actor.player())
                .unwrap()
            else {
                panic!("formal target declaration")
            };
            assert_remaining(
                game,
                actor,
                1,
                wire::AbilityKind::Activated,
                wire::ConsumptionPoint::PlayCommitted,
            );
            let before = exact_authority(game);
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
                .unwrap_err();
            assert_eq!(exact_authority(game), before);
            assert_remaining(
                game,
                actor,
                1,
                wire::AbilityKind::Activated,
                wire::ConsumptionPoint::PlayCommitted,
            );
            let InputDomain::Targets(domain) = question.domain else {
                panic!("targets")
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            assert_remaining(
                game,
                actor,
                0,
                wire::AbilityKind::Activated,
                wire::ConsumptionPoint::PlayCommitted,
            );
            let exhausted = exact_authority(game);
            game.activate_internal_card("s", AbilityId(1), actor.player())
                .unwrap_err();
            assert_eq!(exact_authority(game), exhausted);
        }
        assert_eq!(
            quotas(&games[0], actor, actor),
            quotas(&games[1], actor, actor)
        );
        for game in &mut games {
            play::next_turn(game);
            assert_remaining(
                game,
                actor,
                1,
                wire::AbilityKind::Activated,
                wire::ConsumptionPoint::PlayCommitted,
            );
            play::next_turn(game);
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("s", AbilityId(1), actor.player())
                .unwrap()
            else {
                panic!("next-turn declaration")
            };
            let InputDomain::Targets(domain) = question.domain else {
                panic!("targets")
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            assert_remaining(
                game,
                actor,
                0,
                wire::AbilityKind::Activated,
                wire::ConsumptionPoint::PlayCommitted,
            );
        }
    }
}

#[test]
fn wire_quota_lifecycle_consumes_trigger_at_admission_before_pending_selection() {
    let mut trigger = timing::on_attack(ability(2, vec![draw(1, 0)]));
    trigger.node.trigger.as_mut().unwrap().limit = Some(limit(ConsumptionPoint::PendingAdmission));
    let program = card(vec![trigger]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut games = [
            board(actor, "unit-spell", &program),
            board(actor, "unit-amulet", &program),
        ];
        for game in &mut games {
            assert_remaining(
                game,
                actor,
                1,
                wire::AbilityKind::Triggered,
                wire::ConsumptionPoint::PendingAdmission,
            );
            let target = if actor == Viewer::P1 {
                "P2.leader"
            } else {
                "P1.leader"
            };
            game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":target}))
                .unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            assert_remaining(
                game,
                actor,
                0,
                wire::AbilityKind::Triggered,
                wire::ConsumptionPoint::PendingAdmission,
            );
            let before = exact_authority(game);
            game.answer_internal_card(question.actor, question.token, Answer::Pending(u64::MAX))
                .unwrap_err();
            assert_eq!(exact_authority(game), before);
            let InputDomain::Pending(options) = question.domain else {
                panic!("pending")
            };
            assert_eq!(options.len(), 1);
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            assert_remaining(
                game,
                actor,
                0,
                wire::AbilityKind::Triggered,
                wire::ConsumptionPoint::PendingAdmission,
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
            quick::pass(game);
            play::next_turn(game);
            assert_remaining(
                game,
                actor,
                1,
                wire::AbilityKind::Triggered,
                wire::ConsumptionPoint::PendingAdmission,
            );
        }
        assert_eq!(
            quotas(&games[0], actor, actor),
            quotas(&games[1], actor, actor)
        );
    }
}
