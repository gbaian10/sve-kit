#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these card scenarios."
)]
use super::*;

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

fn trigger(game: &mut Game, printing: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected one card trigger")
    };
    assert_eq!(options.len(), 1, "only the migrated source triggers");
    assert_eq!(
        options[0].card, printing,
        "the option preserves the effective printing"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Resolved(_)),
        "the selected printed trigger resolves"
    );
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
    let mut rejected = super::game(&initial);
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

#[test]
fn etd01_007_alias_evolves_for_one_pp() {
    evolve_for_one("SD04-003", "SD04-004");
}

#[test]
fn etd01_008_alias_evolve_trigger_deals_three_to_the_opposing_follower() {
    let mut initial = field("ETD01-007");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD04-004"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-006"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game, "SD04-004");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 2_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn etd02_004_alias_evolves_for_one_pp_with_its_real_resource() {
    evolve_for_one("SD05-003", "ETD02-005");
}

#[test]
fn etd02_016_alias_quick_deals_three_then_mills_one_before_spell_cleanup() {
    let mut initial = position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"SD05-015"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-006"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick authority window");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "SD05-015");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap();
    assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 2_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["deck"], json!(["P1-next"]));
    assert_eq!(state["P1"]["cemetery"], json!(["P1-top", "spell"]));
    assert_eq!(state["P1"]["hand"], json!([]));
}
