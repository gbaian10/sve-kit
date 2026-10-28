#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these card scenarios."
)]
use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-drawn"),"card":"BP01-037"},
            {"id":format!("{seat}-next"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn field(card: &str) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
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

fn play_follower(initial: &Value, printing: &str) -> Game {
    let mut game = game(initial);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the migrated follower enters for its printed cost"
    );
    trigger(&mut game, printing);
    game
}

fn in_hand(printing: &str) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
    initial
}

fn ward(printing: &str) {
    let mut initial = field(printing);
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "acted Ward prevents bypassing the source to attack its leader"
    );
}

#[test]
fn bp13_013_alias_evolves_for_one_pp() {
    evolve_for_one("EBD01-017", "BP13-014");
}

#[test]
fn bp13_044_alias_evolves_for_one_pp() {
    evolve_for_one("BP13-P10", "BP13-045");
}

#[test]
fn bp13_065_evolves_for_one_pp() {
    evolve_for_one("BP13-065", "BP13-066");
}

#[test]
fn bp13_048_draws_then_discards_then_heals() {
    let game = play_follower(&in_hand("BP13-048"), "BP13-048");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["P1-drawn"]));
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
}

#[test]
fn bp13_061_selects_but_damages_only_when_awakened() {
    for maximum in [6_u8, 7_u8] {
        let mut initial = in_hand("BP13-P14");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-006"}]);
        let game = play_follower(&initial, "BP13-P14");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["hp"],
            if maximum == 7 { 1_u8 } else { 5_u8 },
            "Awakening uses maximum PP after paying the card's four PP"
        );
        assert_eq!(state["P1"]["pp"]["current"], maximum - 4);
    }
}

#[test]
fn bp13_074_alias_lastword_deals_three() {
    let mut initial = field("BP13-P17");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"execution","card":"BP01-169"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-006"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("execution", PlayerId { seat: 2 })
        .unwrap();
    assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
    game.resume_internal_card().unwrap();
    trigger(&mut game, "BP13-P17");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 2_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp13_082_alias_fanfare_mills_the_top_card() {
    let game = play_follower(&in_hand("PR-357"), "PR-357");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["P1-drawn"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-next"]));
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn bp13_087_pays_act_and_self_cemetery_before_damage_and_draw() {
    let mut game = game(&field("BP13-087"));
    let play = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 9_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-drawn"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["field"], json!([]));
}

#[test]
fn bp13_091_ward_protects_its_leader() {
    ward("BP13-091");
}
#[test]
fn bp13_095_ward_protects_its_leader() {
    ward("BP13-095");
}
#[test]
fn bp13_099_ward_protects_its_leader() {
    ward("BP13-099");
}
#[test]
fn bp13_116_ward_protects_its_leader() {
    ward("BP13-116");
}

#[test]
fn bp13_112_alias_fanfare_draws_one() {
    let game = play_follower(&in_hand("BP13-P26"), "BP13-P26");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-drawn"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-next"]));
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}
