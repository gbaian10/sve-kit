#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

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

fn evolution(base: &str, form: &str, cost: u8) -> Game {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution cost permits the matching form"
    );
    game
}

fn activate(game: &mut Game, card: &str) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, actor())
        .unwrap();
    finish(game, step);
    game.resume_internal_card().unwrap();
}

#[test]
fn bp10_078_fanfare_destroys_then_damages_and_heals() {
    let mut game = game(&position("BP10-078"));
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp10_079_evolution_destroys_then_damages_and_heals() {
    let mut game = evolution("BP10-078", "BP10-079", 1);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp10_120_evolution_damages_follower_and_its_leader() {
    let mut game = evolution("BP10-119", "BP10-120", 1);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp10_090_fanfares_check_spells_and_amulets_separately() {
    for (spell, amulet) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut initial = position("BP10-090");
        if spell {
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"spell","card":"BP01-171"}]);
        }
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP10-098"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"],
            if spell { json!([]) } else { json!(["enemy"]) }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if amulet { 15_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp10_071_damages_another_ally_then_buffs_and_can_activate() {
    let mut initial = position("BP10-071");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["damage"], 2_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 4_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    activate(&mut game, "BP10-071");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
        5_u8
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp10_087_evolution_damage_requires_crimson() {
    for crimson in [false, true] {
        let mut initial = position("BP10-086");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-118"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"wolf","card":"BP10-086"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-087"}]);
        let mut game = game(&initial);
        if crimson {
            play(&mut game);
            triggers(&mut game);
        }
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"wolf","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if crimson { 3_u8 } else { 0_u8 }
        );
    }
}

#[test]
fn bp10_034_destroys_discards_the_whole_hand_then_draws_three() {
    let mut initial = position("BP10-034");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"spare","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["cemetery"], json!(["spare"]));
    assert_eq!(state["P1"]["hand"], json!(["first", "second", "third"]));
}

#[test]
fn bp10_107_paid_fanfare_buffs_and_permits_immediate_leader_attack() {
    let mut game = game(&position("BP10-107"));
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(state["objects"]["source"]["power"], 3_u8);
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp10_041_evolution_grants_golem_rush_and_assail() {
    let mut initial = position("BP10-040");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP10-040"},
        {"id":"golem","card":"BP01-T08","state":{"entered_this_turn":true}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-041"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    triggers(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"golem","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"golem","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp10_104_evolution_destroys_up_to_two_enemies() {
    let mut initial = position("BP10-103");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-103"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-104"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second-enemy","card":"BP01-037"}));
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    triggers(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}

#[test]
fn bp10_118_destroys_two_and_heals_only_with_a_fallen_angel() {
    for fallen in [false, true] {
        let mut initial = position("BP10-118");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second-enemy","card":"BP01-037"}));
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"angel","card":if fallen {"BP10-115"} else {"BP01-037"}}]);
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(
            state["P1"]["leader"]["life"],
            if fallen { 22_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp10_098_fanfare_buffs_allies_and_activation_sacrifices_to_debuff() {
    let mut initial = position("BP10-098");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"BP01-037"},{"id":"another","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    for id in ["ally", "another"] {
        assert_eq!(state["objects"][id]["power"], 6_u8);
    }
    activate(&mut game, "BP10-098");
    let activated = game.projection(View::Referee).unwrap();
    assert_eq!(activated["P1"]["cemetery"], json!(["source"]));
    assert_eq!(activated["objects"]["enemy"]["power"], 3_u8);
    assert_eq!(activated["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn required_fanfare_targets_gate_all_following_actions() {
    let mut initial = position("BP10-078");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut no_enemy = game(&initial);
    play(&mut no_enemy);
    assert!(no_enemy.internal_pending_choice().unwrap().is_none());
    let state = no_enemy.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);

    let mut no_ally = game(&position("BP10-071"));
    play(&mut no_ally);
    assert!(no_ally.internal_pending_choice().unwrap().is_none());
    let alone = no_ally.projection(View::Referee).unwrap();
    assert_eq!(alone["objects"]["source"]["power"], 3_u8);
    assert_eq!(alone["objects"]["source"]["hp"], 3_u8);
}
