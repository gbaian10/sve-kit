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

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    checks(game);
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

fn activate_source(game: &mut Game, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && !matches!(
                    &ability.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Evolution,
                        ..
                    }
                )
        })
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(game, step);
    checks(game);
}

fn keyword(state: &Value, object: &str, word: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(word))
}

#[test]
fn bp13_076_fixes_necrocharge_at_resolution_start_before_returning_the_blade() {
    for grave_count in [19_u8, 20, 21] {
        let mut initial = position("BP13-076");
        let mut grave = vec![json!({"id":"blade","card":"BP13-081"})];
        grave.extend(
            (1_u8..grave_count).map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-171"})),
        );
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game, "BP13-076");
        let returned = game.projection(View::Referee).unwrap();
        assert_eq!(returned["P1"]["ex"], json!(["blade"]));
        assert_eq!(returned["objects"]["source"]["power"], 2_u8);
        play(&mut game, "blade");
        let played = game.projection(View::Referee).unwrap();
        assert_eq!(
            played["P1"]["pp"]["current"],
            if grave_count >= 20 { 8_u8 } else { 6 }
        );
        assert_eq!(played["objects"]["enemy"]["hp"], 1_u8);
        assert_eq!(
            played["P2"]["leader"]["life"],
            if grave_count == 21 { 16_u8 } else { 20 }
        );
    }
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
fn bp14_109_finds_either_angel_trait_and_puts_only_the_remainder_on_the_bottom() {
    for wanted in ["BP01-159", "BP03-118"] {
        let mut initial = evolution_position("BP14-108", "BP14-109");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"picked","card":wanted},{"id":"miss-a","card":"BP01-171"},
            {"id":"miss-b","card":"BP01-171"},{"id":"miss-c","card":"BP01-171"},
            {"id":"tail","card":"BP01-171"}
        ]);
        let mut game = game(&initial);
        evolve_source(&mut game, 1);
        trigger(&mut game, "BP14-109");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["picked"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["tail", "miss-c", "miss-b", "miss-a"])
        );
    }
}

#[test]
fn bp12_089_revives_a_cheap_machine_and_exchanges_two_repairs_for_storm() {
    let mut initial = position("BP12-089");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"revived","card":"BP07-110"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"repair-a","card":"BP07-T02"},{"id":"repair-b","card":"BP07-T02"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP12-089");
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["field"], json!(["source", "revived"]));
    assert!(!keyword(&entered, "source", "疾走"));
    activate_source(&mut game, "BP12-089");
    let granted = game.projection(View::Referee).unwrap();
    assert!(keyword(&granted, "source", "疾走"));
    assert_eq!(granted["P1"]["ex"], json!([]));
    // Tokens cease to exist outside the field and EX area.
    assert_eq!(granted["P1"]["banish"], json!([]));
}

#[test]
fn bp12_090_buffs_only_the_other_machine_in_its_second_evolution_mode() {
    let mut initial = evolution_position("BP12-089", "BP12-090");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP12-089"},{"id":"machine","card":"BP07-110"},
        {"id":"plain","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    evolve_source(&mut game, 1);
    trigger_mode(&mut game, "BP12-090", ModeId(2));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["machine"]["power"], 3_u8);
    assert_eq!(state["objects"]["machine"]["hp"], 3_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 5_u8);
    assert_eq!(state["objects"]["plain"]["power"], 5_u8);
}

#[test]
fn bp12_060_revives_a_low_cost_dragon_and_buffs_it_with_storm() {
    let mut initial = evolution_position("BP12-059", "BP12-060");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"dragon","card":"BP01-092"}]);
    let mut game = game(&initial);
    evolve_source(&mut game, 2);
    trigger(&mut game, "BP12-060");
    activate_source(&mut game, "BP12-060");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "dragon"]));
    assert_eq!(state["objects"]["dragon"]["power"], 2_u8);
    assert_eq!(state["objects"]["dragon"]["hp"], 2_u8);
    assert!(keyword(&state, "dragon", "疾走"));
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn bp13_109_pays_with_an_ally_and_observes_both_the_payment_and_its_own_death() {
    let mut initial = evolution_position("BP13-108", "BP13-109");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP13-108"},{"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"soul","card":"BP01-116"}]);
    let mut game = game(&initial);
    evolve_source(&mut game, 2);
    trigger(&mut game, "BP13-109");
    let paid = game.projection(View::Referee).unwrap();
    assert_eq!(paid["P2"]["field"], json!(["enemy"]));
    assert_eq!(paid["P1"]["cemetery"], json!(["ally"]));
    assert_eq!(paid["P2"]["leader"]["life"], 20_u8);
    assert_eq!(paid["P1"]["field"].as_array().unwrap().len(), 2_usize);
    trigger(&mut game, "BP13-109");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19_u8
    );
    play(&mut game, "soul");
    trigger(&mut game, "BP13-109");
    let observed = game.projection(View::Referee).unwrap();
    assert_eq!(observed["P2"]["leader"]["life"], 18_u8);
    assert!(
        observed["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
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

#[test]
fn bp06_079_uses_necrocharge_for_the_sacrifice_count_and_can_fire_from_hand() {
    for grave_count in [0_u8, 10] {
        let mut initial = position("BP06-079");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..grave_count)
                .map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037"},{"id":"enemy-two","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game, "BP06-079");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"].as_array().unwrap().len(),
            usize::from(grave_count != 10)
        );
    }
    let mut game = game(&position("BP06-079"));
    activate_source(&mut game, "BP06-079");
    let fired = game.projection(View::Referee).unwrap();
    assert_eq!(fired["P1"]["pp"]["current"], 7_u8);
    assert_eq!(fired["P2"]["field"], json!([]));
    assert_eq!(fired["P1"]["cemetery"], json!(["source", "P1-first"]));
}
