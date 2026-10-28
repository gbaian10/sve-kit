#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-037"},
            {"id":format!("{seat}-second"),"card":"BP01-037"},
            {"id":format!("{seat}-third"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn field_position(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected target decision");
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

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", actor()).unwrap();
    finish(game, result);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending trigger");
    };
    assert_eq!(options.len(), 1, "one card trigger is pending");
    let result = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, result);
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let result = game
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish(game, result);
}

fn evolve(card: &str, form: &str, cost: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the exact printed cost permits evolution"
    );
    let event = step
        .events
        .iter()
        .find(|event| event["kind"] == "進化")
        .unwrap();
    assert_eq!(event["object"], "source", "the real base evolves");
    assert_eq!(
        event["with"], "form",
        "the matching evolution resource is used"
    );
}

fn ward(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":card,"state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "rested ward protects the leader"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"ward"}))
            .unwrap()
            .outcome,
        "resolved",
        "the ward remains attackable"
    );
}

fn immediate_attack(card: &str, keyword: &str, power: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    let target = if keyword == "storm" {
        "P2.leader"
    } else {
        "enemy"
    };
    if keyword == "rush" {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack",
            "rush does not allow an immediate leader attack"
        );
    }
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed keyword permits the immediate attack"
    );
    pass_quick(&mut game);
    if keyword == "storm" {
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8.checked_sub(power).unwrap(),
            "storm damages the opposing leader immediately"
        );
    } else if power >= 5 {
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"]),
            "the opposing follower is destroyed"
        );
    } else {
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            power,
            "rush deals combat damage to its defender"
        );
    }
}

fn departed(card: &str) -> Game {
    let mut game = with_producer(
        &field_position(card),
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    game
}

fn count_named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp08_090_lastword_damages_opposing_leader() {
    let game = departed("BP08-090");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp08_027_rush_attack_triggers_four_damage() {
    let mut initial = field_position("BP08-027");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "rush permits attacking on entry"
    );
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
}

#[test]
fn bp08_067_rush_and_own_end_destruction() {
    immediate_attack("BP08-067", "rush", 7);
    let mut game = game(&field_position("BP08-067"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the own end phase begins"
    );
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["source"])
    );
}

#[test]
fn bp08_008_evolves_for_two_pp() {
    evolve("BP08-008", "BP08-009", 2);
}

#[test]
fn bp08_019_storm_attacks_on_entry() {
    immediate_attack("BP08-019", "storm", 3);
}

#[test]
fn bp08_025_ward_protects_its_leader() {
    ward("BP08-025");
}

#[test]
fn bp08_042_evolves_for_two_pp() {
    evolve("BP08-042", "BP08-043", 2);
}

#[test]
fn bp08_093_evolves_for_two_pp() {
    evolve("BP08-093", "BP08-094", 2);
}

#[test]
fn bp08_099_evolves_for_two_pp() {
    evolve("BP08-099", "BP08-100", 2);
}

#[test]
fn bp08_t03_rush_attacks_follower_on_entry() {
    immediate_attack("BP08-T03", "rush", 2);
}

#[test]
fn bp08_044_fanfare_draws_before_discarding() {
    let mut game = game(&position("BP08-044"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!([]),
        "the only drawn card must be discarded"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["P1-first"]),
        "discard can select the card just drawn"
    );
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
}

#[test]
fn bp08_045_quick_spell_draws_two_before_discarding() {
    let mut game = game(&position("BP08-045"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        1,
        "one drawn card remains"
    );
    assert_eq!(
        state["P1"]["cemetery"].as_array().unwrap().len(),
        2,
        "discard and resolved spell are in cemetery"
    );
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
}

#[test]
fn bp08_081_fanfare_mills_top_two() {
    let mut game = game(&position("BP08-081"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["P1-first", "P1-second"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
}

#[test]
fn bp08_012_fanfare_creates_two_admitted_puppets() {
    let mut game = game(&position("BP08-012"));
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        count_named(&game.projection(View::Referee).unwrap(), "ex", "操り人形"),
        2,
        "two puppet tokens enter EX"
    );
}

#[test]
fn bp08_013_attack_time_creates_a_fairy_wisp() {
    let mut game = game(&field_position("BP08-013"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the follower can attack normally"
    );
    trigger(&mut game);
    assert_eq!(
        count_named(
            &game.projection(View::Referee).unwrap(),
            "ex",
            "フェアリーウィスプ"
        ),
        1,
        "the attack creates one fairy wisp"
    );
}
