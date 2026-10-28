#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these card scenarios."
)]
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "rescan-036",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-top"),"card":"BP01-037"},
            {"id":format!("{seat}-next"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn field(printing: &str) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected target choice");
            };
            assert_eq!(
                domain.members.len(),
                1,
                "exactly the intended opponent is eligible"
            );
            Answer::Targets(vec![domain.members[0].handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn evolve_for_one(base: &str, form: &str) {
    let mut initial = field(base);
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the printed one PP evolves the real base"
    );
    let event = step
        .events
        .iter()
        .find(|event| event["kind"] == "進化")
        .unwrap();
    assert_eq!(event["object"], "source", "the base holder evolves");
    assert_eq!(event["with"], "form", "the real evolution resource is used");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8,
        "the printed evolution cost consumes the last PP"
    );
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    let mut rejected = self::game(&initial);
    assert_eq!(
        rejected
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "cannot-evolve",
        "evolution cannot bypass the printed PP cost"
    );
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn jun_alias_evolves_for_one_and_ward_protects_the_leader() {
    evolve_for_one("CP04-P15", "CP04-024");
    let mut initial = field("CP04-P15");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"P1.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    pass_quick(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["objects"]["enemy"]["hp"], 2_u8);
}

#[test]
fn tamaki_alias_evolves_for_one_and_storm_attacks_on_entry() {
    evolve_for_one("CP04-P23", "CP04-032");
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP04-P23"}]);
    let mut game = game(&initial);
    let step = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(finish(&mut game, step), CardPlay::Entered(_)));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    pass_quick(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn yui_alias_evolves_for_one_pp_and_rejects_short_payment() {
    evolve_for_one("PCS01-041", "CP04-096");
}
