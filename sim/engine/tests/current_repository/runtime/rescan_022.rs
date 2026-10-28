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

fn finish(game: &mut Game, mut play: CardPlay, accept_optional: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(accept_optional)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
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
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_) | CardPlay::Entered(_) | CardPlay::Continued(_)
        ),
        "the effect completes"
    );
}

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", actor()).unwrap();
    finish(game, result, true);
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
    finish(game, result, true);
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let result = game
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish(game, result, true);
}

fn evolve(card: &str, form: &str, cost: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    if let Some(less) = cost.checked_sub(1) {
        let mut insufficient = initial.clone();
        insufficient["players"]["P1"]["pp"]["current"] = json!(less);
        let mut denied = game(&insufficient);
        if let Ok(step) = denied.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":less}}),
        ) {
            assert_ne!(
                step.outcome, "resolved",
                "less than the printed cost cannot evolve"
            );
        }
        assert_eq!(
            denied.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            less,
            "rejected evolution preserves PP"
        );
    }
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

fn storm_attack(card: &str, power: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "storm permits an immediate leader attack"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8.checked_sub(power).unwrap(),
        "storm deals the printed attack power"
    );
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

#[test]
fn bp16_005_pays_one_pp_to_evolve() {
    evolve("BP16-005", "BP16-006", 1);
}

#[test]
fn bp16_023_pays_one_pp_to_evolve() {
    evolve("BP16-023", "BP16-024", 1);
}

#[test]
fn bp16_027_pays_one_pp_to_evolve() {
    evolve("BP16-027", "BP16-028", 1);
}

#[test]
fn bp16_042_pays_one_pp_to_evolve() {
    evolve("BP16-042", "BP16-043", 1);
}

#[test]
fn bp16_050_pays_one_pp_to_evolve() {
    evolve("BP16-050", "BP16-051", 1);
}

#[test]
fn bp16_061_pays_one_pp_to_evolve() {
    evolve("BP16-061", "BP16-062", 1);
}

#[test]
fn bp16_031_evolves_and_attacks_on_entry() {
    evolve("BP16-031", "BP16-032", 1);
    storm_attack("BP16-031", 1);
}
#[test]
fn bp16_065_evolves_storms_and_cannot_be_attacked_by_followers() {
    evolve("BP16-065", "BP16-066", 1);
    storm_attack("BP16-065", 2);
    let mut initial = field_position("BP01-037");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP16-065","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}
#[test]
fn bp16_063_storm_deals_seven_damage_immediately() {
    storm_attack("BP16-063", 7);
}
#[test]
fn bp16_087_fanfare_damages_only_opposing_followers() {
    let mut initial = position("BP16-087");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy1","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"},{"id":"amulet","card":"BP01-016"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    for enemy in ["enemy1", "enemy2"] {
        assert_eq!(state["objects"][enemy]["damage"], 3_u8);
    }
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["objects"]["amulet"]["damage"], 0_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    evolve("BP16-087", "BP16-088", 1);
    storm_attack("BP16-087", 4);
}
#[test]
fn bp16_106_printed_ward_redirects_attacks() {
    ward("BP16-106");
}
#[test]
fn bp16_091_entry_hurts_and_attack_heals_its_controller() {
    let mut entry = game(&position("BP16-091"));
    play(&mut entry);
    trigger(&mut entry);
    let state = entry.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    let mut attack = game(&field_position("BP16-091"));
    assert_eq!(
        attack
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut attack);
    assert_eq!(
        attack.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        25_u8
    );
}
#[test]
fn bp16_111_fanfare_draws_and_ward_protects() {
    let mut game = game(&position("BP16-111"));
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["P1-first"])
    );
    ward("BP16-111");
    evolve("BP16-111", "BP16-112", 1);
}
#[test]
fn bp16_117_lastword_heals_then_draws() {
    ward("BP16-117");
    let game = departed("BP16-117");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}
#[test]
fn bp16_t01_lastword_heals_even_after_token_ceases_to_exist() {
    ward("BP16-T01");
    let game = departed("BP16-T01");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert!(
        !state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the departed token does not remain in the cemetery"
    );
}
#[test]
fn ebd01_010_printing_shares_the_evolution_program() {
    evolve("EBD01-010", "BP16-006", 1);
}
