#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-171"},
            {"id":format!("{seat}-second"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, step: CardPlay) {
    finish_mode(game, step, ModeId(1));
}

fn finish_mode(game: &mut Game, mut step: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            let mut members: Vec<_> = domain.members.iter().collect();
            members.sort_by_key(|member| {
                if member.card.as_deref() == Some("BP01-092") {
                    0_u8
                } else if member.card.as_deref() == Some("BP01-037") {
                    2
                } else {
                    3
                }
            });
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::Targets(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &q.domain {
            let members: Vec<_> = domain.members.iter().collect();
            assert!(
                members.iter().all(|member| member.player == actor()),
                "Only the controller can supply these cost materials"
            );
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::CostMaterials(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if matches!(&q.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![mode])
        } else if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game, card: &str) {
    trigger_mode(game, card, ModeId(1));
}

fn trigger_mode(game: &mut Game, card: &str, mode: ModeId) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("expected a pending ability")
    };
    let chosen = options.iter().find(|option| option.card == card).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish_mode(game, step, mode);
    checks(game);
}

fn evolution_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve_source(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "The printed evolution is legal in this position"
    );
    checks(game);
}

#[test]
fn bp13_020_counts_the_follower_just_destroyed_and_requires_ten_levion_to_stand() {
    for count in [9_u8, 10] {
        let mut initial = position("BP13-020");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-020"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..count)
                .map(|i| json!({"id":format!("levion-{i}"),"card":"BP13-032"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game, "BP13-020");
        let attacking = game.projection(View::Referee).unwrap();
        assert_eq!(attacking["P2"]["field"], json!([]));
        assert_eq!(attacking["objects"]["source"]["power"], 5_u8);
        assert_eq!(attacking["objects"]["source"]["hp"], 7_u8);
        for _ in 0..2_u8 {
            let Some(q) = game.internal_quick_choice().unwrap() else {
                break;
            };
            let step = game
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(&mut game, step);
            checks(&mut game);
        }
        let ability = selection().current["BP13-020"]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let result = game.activate_internal_card("source", ability, actor());
        if count == 10 {
            finish(&mut game, result.unwrap());
            let stood = game.projection(View::Referee).unwrap();
            assert_eq!(stood["objects"]["source"]["acted"], false);
            assert_eq!(stood["P1"]["pp"]["current"], 8_u8);
        } else {
            result.unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                10_u8
            );
        }
    }
}

#[test]
fn bp18_014_requires_a_real_prior_effect_evolution_and_then_evolves_for_zero_pp() {
    let mut initial = evolution_position("BP18-014", "BP18-015");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"effect-base","card":"BP06-005"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"effect-form","card":"BP06-006"}));
    let entry = Action::Move {
        objects: subjects_in(ZoneKind::Cemetery),
        movement: MoveSpec {
            to: MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                },
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    };
    let mut game = with_producer(&initial, entry);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}})
        )
        .unwrap()
        .outcome,
        "cannot-evolve"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8
    );
    activate(&mut game, "P1");
    checks(&mut game);
    trigger(&mut game, "BP06-005");
    trigger(&mut game, "BP06-006");
    evolve_source(&mut game, 0);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["objects"]["source"]["hp"], 2_u8);
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
}
