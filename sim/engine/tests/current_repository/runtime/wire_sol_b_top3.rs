#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

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
        } else if let InputDomain::Search(domain) = &q.domain {
            let limit = usize::try_from(
                domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0,
            )
            .unwrap();
            Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(limit).map(|c| c.handle).collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if matches!(&q.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![mode])
        } else if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::TokenCapacity { .. }) {
            Answer::TokenCapacity(0)
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
                        keyword: KeywordId::Evolution | KeywordId::Meal | KeywordId::Ride,
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

fn add_evolver(initial: &mut Value) {
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"evolver","card":"BP01-135"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-136"}]);
}

fn evolve_ally(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"evolver","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "The helper evolution must record a real evolution this turn"
    );
    checks(game);
    trigger(game, "BP01-136");
}

#[test]
fn bp06_105_plays_off_turn_and_uses_four_damage_with_a_friendly_ward() {
    let mut initial = position("BP06-105");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ward","card":"BP01-046"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(q.actor, actor());
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick window")
    };
    let chosen = options.iter().find(|o| o.card == "BP06-105").unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(Some(chosen.handle)))
        .unwrap();
    finish(&mut game, step);
    checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp14_029_recruits_from_five_cards_and_can_discard_itself_from_hand_for_a_knight() {
    let mut initial = position("BP14-029");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"BP01-034"},{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},{"id":"c","card":"BP01-171"},{"id":"d","card":"BP01-171"}]);
    let mut field_game = game(&initial);
    play(&mut field_game, "source");
    trigger(&mut field_game, "BP14-029");
    let field_state = field_game.projection(View::Referee).unwrap();
    assert_eq!(field_state["P1"]["field"], json!(["source", "picked"]));
    assert_eq!(field_state["P1"]["deck"], json!(["d", "c", "b", "a"]));
    assert!(keyword(&field_state, "source", "守護"));
    let mut hand_game = game(&position("BP14-029"));
    activate_source(&mut hand_game, "BP14-029");
    let hand_state = hand_game.projection(View::Referee).unwrap();
    assert_eq!(hand_state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(hand_state["P1"]["pp"]["current"], 9_u8);
    let knight = hand_state["P1"]["field"][0].as_str().unwrap();
    assert_eq!(hand_state["objects"][knight]["power"], 2_u8);
    assert_eq!(hand_state["objects"][knight]["hp"], 2_u8);
}

#[test]
fn bp12_017_adds_and_reveals_a_fairy_then_pays_itself_to_heal_with_three_ex_fairies() {
    let mut initial = position("BP12-017");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"f1","card":"BP01-T01"},{"id":"f2","card":"BP01-T01"},{"id":"f3","card":"BP01-T01"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"BP01-013"},{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP12-017");
    assert!(
        game.projection(View::P2).unwrap()["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "picked"),
        "The added card is revealed"
    );
    activate_source(&mut game, "BP12-017");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["picked"]));
    assert_eq!(state["P1"]["deck"], json!(["b", "a"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
}

#[test]
fn cp03_003_acts_an_enemy_then_recruits_two_aqua_force_followers_on_its_own_turn_death() {
    let mut initial = position("CP03-003");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"kill","card":"BP01-116"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"},{"id":"aqua1","card":"CP03-015"},{"id":"aqua2","card":"CP03-015"},{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},{"id":"c","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "CP03-003");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
        true
    );
    play(&mut game, "kill");
    trigger(&mut game, "CP03-003");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["aqua1", "aqua2"]));
    assert_eq!(state["P1"]["deck"], json!(["c", "b", "a"]));
}

#[test]
fn bp16_110_destroys_on_entry_and_uses_a_real_evolution_to_enable_damage_and_draw() {
    let mut initial = position("BP16-110");
    add_evolver(&mut initial);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP16-110");
    evolve_ally(&mut game);
    activate_source(&mut game, "BP16-110");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1_usize);
}

#[test]
fn bp12_046_creates_a_sigil_and_spends_its_stack_to_buff_another_golem() {
    let mut initial = position("BP12-046");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"golem","card":"BP02-T14"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP12-046");
    activate_source(&mut game, "BP12-046");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["golem"]["power"], 3_u8);
    assert_eq!(state["objects"]["golem"]["hp"], 4_u8);
    assert_eq!(state["P1"]["field"], json!(["golem", "source"]));
}

#[test]
fn bp16_092_mills_then_sacrifices_after_a_real_evolution_to_create_a_storm_ghost() {
    let mut initial = position("BP16-092");
    add_evolver(&mut initial);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP16-092");
    evolve_ally(&mut game);
    activate_source(&mut game, "BP16-092");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["P1-first", "source"]));
    let ghost = state["P1"]["field"][1].as_str().unwrap();
    assert!(keyword(&state, ghost, "疾走"));
}

#[test]
fn bp14_065_puts_a_small_dragon_in_ex_and_buffs_that_same_card() {
    let mut initial = evolution_position("BP14-064", "BP14-065");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"BP01-092"},{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},{"id":"c","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve_source(&mut game, 2);
    trigger(&mut game, "BP14-065");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["picked"]));
    assert_eq!(state["P1"]["deck"], json!(["c", "b", "a"]));
    assert_eq!(state["objects"]["picked"]["power"], 2_u8);
    assert_eq!(state["objects"]["picked"]["hp"], 2_u8);
}

#[test]
fn bp03_046_buffs_another_chess_follower_after_its_real_entry() {
    let mut initial = position("BP03-046");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"watcher","card":"BP03-046"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP03-046");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["watcher"]["power"], 3_u8);
}

#[test]
fn bp03_047_creates_a_pawn_after_evolution_and_observes_its_entry() {
    let mut game = game(&evolution_position("BP03-046", "BP03-047"));
    evolve_source(&mut game, 1);
    trigger(&mut game, "BP03-047");
    trigger(&mut game, "BP03-047");
    let state = game.projection(View::Referee).unwrap();
    let pawn = state["P1"]["field"][1].as_str().unwrap();
    assert_eq!(state["objects"][pawn]["power"], 3_u8);
    assert_eq!(state["objects"][pawn]["hp"], 1_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
}

#[test]
fn bp16_011_uses_a_real_follower_death_to_enable_play_and_four_damage() {
    let mut initial = position("BP16-011");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"victim","card":"BP01-034"}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"kill","card":"BP01-116"}));
    let mut game = game(&initial);
    play(&mut game, "kill");
    play(&mut game, "source");
    trigger(&mut game, "BP16-011");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["victim", "kill"]));
}

#[test]
fn sd06_015_pays_pp_and_itself_to_create_a_storm_falcon() {
    let mut game = game(&position("SD06-015"));
    play(&mut game, "source");
    activate_source(&mut game, "SD06-015");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    let falcon = state["P1"]["field"][0].as_str().unwrap();
    assert!(keyword(&state, falcon, "疾走"));
    assert_eq!(state["objects"][falcon]["power"], 2_u8);
}

#[test]
fn cp03_058_banishes_itself_to_stand_another_pale_moon_and_prohibit_leader_attacks() {
    let mut initial = position("CP03-058");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-058"},{"id":"ally","card":"CP03-058","state":{"acted":true}}]);
    let mut game = game(&initial);
    activate_source(&mut game, "CP03-058");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["source"]));
    assert_eq!(state["objects"]["ally"]["acted"], false);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
}
