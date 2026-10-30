#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Distribution(domain) = &question.domain {
            assert_eq!(
                domain.members.len(),
                2,
                "the scenario selects two opposing followers"
            );
            Answer::Distribution(vec![
                DamageShare {
                    handle: domain.members[0].handle,
                    amount: Nat(domain.total.0.checked_sub(1).unwrap()),
                },
                DamageShare {
                    handle: domain.members[1].handle,
                    amount: Nat(1),
                },
            ])
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step);
}

fn triggers(game: &mut Game) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
        game.resume_internal_card().unwrap();
    }
}

fn pass_quick(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

fn evolve(game: &mut Game, form: &str, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":form,"pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
    );
    triggers(game);
}
fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

#[test]
fn bp16_120_deals_seven_to_follower_and_three_to_its_leader() {
    let mut game = game(&position("BP16-120"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}
#[test]
fn bp16_118_fanfare_damages_all_opposing_followers_and_leader() {
    let mut initial = position("BP16-118");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"small","card":"BP01-171"}));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["small"]["damage"], 1_u8);
    assert_eq!(state["P2"]["field"], json!(["enemy", "small"]));
    assert_eq!(state["P2"]["cemetery"], json!([]));
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
}
#[test]
fn bp16_119_real_evolution_damages_all_opposing_followers_and_leader() {
    let mut initial = evolved_position("BP16-118", "BP16-119");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"small","card":"BP01-171"}));
    let mut game = game(&initial);
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["small"]["damage"], 1_u8);
    assert_eq!(state["P2"]["field"], json!(["enemy", "small"]));
    assert_eq!(state["P2"]["cemetery"], json!([]));
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
}
#[test]
fn bp16_032_real_evolution_can_target_opposing_leader_or_follower() {
    for leader in [false, true] {
        let mut game = game(&evolved_position("BP16-031", "BP16-032"));
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the matching real form uses the printed cost"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution trigger")
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("recipient choice")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal target")
        };
        let chosen = domain
            .members
            .iter()
            .find(|m| m.card.is_none() == leader)
            .unwrap()
            .handle;
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![chosen]),
            )
            .unwrap();
        finish(&mut game, step);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if leader { 19_u8 } else { 20_u8 }
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            u8::from(!leader)
        );
    }
}
#[test]
fn bp16_094_own_end_phase_healing_requires_two_amulets() {
    for amulets in [1_usize, 2] {
        let mut initial = position("BP16-094");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-094"}]);
        for n in 0..amulets {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("amulet{n}"),"card":"BP01-016"}));
        }
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "the controller ends its turn"
        );
        loop {
            triggers(&mut game);
            if game.internal_quick_choice().unwrap().is_none() {
                break;
            }
            pass_quick(&mut game);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if amulets == 2 { 22_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp16_007_sets_follower_power_and_hp_to_one() {
    let mut game = game(&position("BP16-007"));
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 1_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
}
#[test]
fn bp16_086_destroys_one_enemy_and_one_own_follower_then_draws() {
    let mut initial = position("BP16-086");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["cemetery"], json!(["ally", "source"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}
#[test]
fn bp16_056_pp_maximum_ten_adds_power_and_aura() {
    for maximum in [9_u8, 10] {
        let mut initial = position("BP16-056");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if maximum == 10 { 7_u8 } else { 6_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("オーラ")),
            maximum == 10
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "printed Storm permits an entry-turn leader attack"
        );
    }
}
#[test]
fn bp16_048_discards_only_an_academy_card_before_drawing_two() {
    let mut initial = position("BP16-048");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"academy","card":"BP16-048"}));
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"wrong","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["academy"]));
    assert_eq!(state["P1"]["hand"], json!(["wrong", "first", "second"]));
}
#[test]
fn bp16_030_storm_combat_death_searches_another_copy_into_hand() {
    let mut initial = position("BP16-030");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"copy","card":"BP16-030"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Storm permits the entry-turn combat attack"
    );
    pass_quick(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["copy"]));
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}
#[test]
fn bp16_013_fanfare_creates_one_fairy_on_field() {
    let mut game = game(&position("BP16-013"));
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
    let fairy = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "フェアリー")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(state["objects"][fairy]["power"], 1_u8);
    assert_eq!(state["objects"][fairy]["hp"], 1_u8);
}
