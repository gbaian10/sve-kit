#![expect(
    clippy::panic,
    reason = "Unexpected choices fail these game scenarios."
)]

use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},{"id":"next","card":"BP01-037"}
    ]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected a field target");
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "only the migrated card trigger is pending"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

fn play(game: &mut Game) {
    let entered = game.play_internal_card("source", actor()).unwrap();
    finish(game, entered);
}

#[test]
fn bp01_118_fanfare_hits_both_leaders() {
    let mut game = game(&position("BP01-118"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u64);
    assert_eq!(state["P2"]["leader"]["life"], 19_u64);
}

#[test]
fn bp01_108_fanfare_and_sacrifice_each_damage_self_and_draw() {
    let mut game = game(&position("BP01-108"));
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let activated = game
        .activate_internal_card("source", AbilityId(4), actor())
        .unwrap();
    finish(&mut game, activated);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u64);
    assert_eq!(state["P2"]["leader"]["life"], 20_u64);
    assert_eq!(state["P1"]["hand"], json!(["drawn", "next"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u64);
}

fn evolve(base: &str, form: &str, cost: u8) -> Game {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base}, {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed PP cost permits evolution"
    );
    trigger(&mut game);
    game
}

#[test]
fn bp01_160_evolve_damages_enemy_and_lastword_draws() {
    let mut evolved_game = evolve("BP01-159", "BP01-160", 2);
    assert_eq!(
        evolved_game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u64
    );
    evolved_game.resume_internal_card().unwrap();
    let mut initial = position("BP01-160");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-160"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn"])
    );
}

#[test]
fn bp01_064_hits_all_enemies_and_leaves_allies_alone() {
    let mut initial = position("BP01-064");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"a","card":"BP01-037"},{"id":"b","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["a"]["damage"], 4_u64);
    assert_eq!(state["objects"]["b"]["damage"], 4_u64);
    assert_eq!(state["objects"]["ally"]["damage"], 0_u64);
    assert_eq!(state["P2"]["leader"]["life"], 20_u64);
}

#[test]
fn bp01_127_fanfare_hits_opposing_followers() {
    let mut initial = position("BP01-127");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u64);
    assert_eq!(state["objects"]["source"]["damage"], 0_u64);
}

#[test]
fn bp01_128_evolve_hits_enemies_and_increases_only_other_allies_health() {
    let game = evolve("BP01-127", "BP01-128", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u64);
    assert_eq!(state["objects"]["ally"]["hp"], 7_u64);
    assert_eq!(state["objects"]["source"]["hp"], 5_u64);
}

#[test]
fn bp01_153_own_end_phase_hits_only_opponent_leader() {
    for active in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(active);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-153"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "ending the phase resolves"
        );
        if active == "P1" {
            trigger(&mut game);
        } else {
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 20_u64);
        assert_eq!(
            state["P2"]["leader"]["life"],
            if active == "P1" { 16_u64 } else { 20_u64 }
        );
    }
}

#[test]
fn bp01_006_fanfare_then_paid_activation_damage_enemy() {
    let mut initial = position("BP01-006");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-153"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let activated = game
        .activate_internal_card("source", AbilityId(4), actor())
        .unwrap();
    finish(&mut game, activated);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 8_u64);
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P1"]["pp"]["current"], 4_u64);
}

#[test]
fn bp01_159_lastword_draws_after_destruction() {
    let mut initial = position("BP01-159");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-159"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn"])
    );
}

#[test]
fn bp01_016_sacrifice_heals_and_its_departure_draws() {
    let mut initial = position("BP01-016");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-016"}]);
    let mut game = game(&initial);
    let activated = game
        .activate_internal_card("source", AbilityId(4), actor())
        .unwrap();
    finish(&mut game, activated);
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u64);
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u64);
}

#[test]
fn bp01_147_fanfare_heals_five_and_draws_one() {
    let mut game = game(&position("BP01-147"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 25_u64);
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
}
