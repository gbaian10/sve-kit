#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn in_hand(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn on_field(card: &str) -> Value {
    let mut initial = in_hand(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(take(&domain.cardinality, domain.cards.len()))
                    .map(|card| card.handle)
                    .collect(),
            )),
            InputDomain::Distribution(domain) => Answer::Distribution(vec![DamageShare {
                handle: domain.members[0].handle,
                amount: domain.total,
            }]),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|card| card.handle).collect())
            }
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()).max(1))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers_with(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
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
        finish_with(game, step, modes);
    }
}

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
}

fn play(game: &mut Game) {
    play_with(game, "source", &[1]);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers_with(game, &[1]);
}

fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    loop {
        triggers_with(game, &[1]);
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_with(game, step, &[1]);
    }
}

fn acted_enemy(initial: &mut Value) {
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
}

#[test]
fn bp10_001_takes_no_ability_damage() {
    let mut initial = on_field("BP10-001");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP05-068"}]);
    let mut game = game(&initial);
    // The spell must hit one own follower, which ignores the damage.
    play_with(&mut game, "spell", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn bp10_002_evolution_grows_with_combo_three_and_keeps_its_immunity() {
    let mut initial = on_field("BP10-001");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-002"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"g1","card":"BP01-171"},{"id":"g2","card":"BP01-171"},{"id":"g3","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    for id in ["g1", "g2", "g3"] {
        play_with(&mut game, id, &[1]);
    }
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 4_u8);
}

#[test]
fn bp14_069_awakened_places_an_ocean_card_in_ex_one_cheaper() {
    let mut initial = in_hand("BP14-069");
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"whale","card":"BP04-069"},
        {"id":"other","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    assert_eq!(placed["P1"]["ex"], json!(["whale"]));
    assert_eq!(placed["P1"]["pp"]["current"], 6_u8);
    play_with(&mut game, "whale", &[1]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8,
        "the five-cost whale costs four"
    );
}

#[test]
fn bp15_063_evolution_places_two_ocean_cards_cheaper_and_pings_on_ocean_entry() {
    let mut initial = on_field("BP15-062");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-063"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"whale","card":"BP04-069"},{"id":"goblin","card":"BP01-171"},
        {"id":"whale2","card":"BP04-069"},{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    assert_eq!(placed["P1"]["ex"], json!(["whale", "whale2"]));
    let before = placed["P1"]["pp"]["current"].as_u64().unwrap();
    play_with(&mut game, "whale", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], json!(before.checked_sub(3).unwrap()));
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp14_093_finds_a_zealot_cheaper_and_strikes_when_healed() {
    let mut initial = in_hand("BP14-093");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"zealot","card":"BP14-101"},
        {"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},{"id":"c","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fox","card":"BP14-T06"}]);
    let mut game = game(&initial);
    play(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    assert!(placed["P1"]["ex"].as_array().unwrap().contains(&json!("zealot")));
    let before = placed["P1"]["pp"]["current"].clone();
    play_with(&mut game, "zealot", &[1]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before,
        "the two-cost zealot is free"
    );
    // The fox heals one on entry, which triggers four damage.
    play_with(&mut game, "fox", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp15_044_evolution_deals_two() {
    let mut initial = on_field("BP15-043");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-044"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8
    );
}

#[test]
fn bp14_105_deals_four_and_evolves_with_three_revelers() {
    let mut initial = in_hand("BP14-105");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"reveler","card":"BP14-060"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"coin","card":"BP14-T02"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-106"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
    assert_eq!(state["objects"]["enemy"]["damage"], 9_u8, "the evolution adds five");
}

#[test]
fn bp14_106_cannot_evolve_without_three_revelers() {
    let mut initial = on_field("BP14-105");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-106"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "cannot-evolve"
    );
}

#[test]
fn bp13_005_attack_weakens_the_target_and_grows_with_five_elf_spell_names() {
    let mut initial = on_field("BP13-005");
    acted_enemy(&mut initial);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"s1","card":"BP01-007"},{"id":"s2","card":"BP01-015"},{"id":"s3","card":"BP01-023"},
        {"id":"s4","card":"BP01-024"},{"id":"s5","card":"BP02-011"}
    ]);
    let mut game = game(&initial);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    // 4/4 after its buff; the 3/3 sage takes four and dies.
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp14_091_cannot_be_played_from_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP14-091"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("source", actor()).unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn bp15_075_lastword_returns_another_ocean_follower_to_ex() {
    let mut initial = on_field("BP15-075");
    acted_enemy(&mut initial);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true,"power":9_u8});
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"twin","card":"BP15-075"},{"id":"whale","card":"BP04-069"}
    ]);
    let mut game = game(&initial);
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["whale"]));
}
