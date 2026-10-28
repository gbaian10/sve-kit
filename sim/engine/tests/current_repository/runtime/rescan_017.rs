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

fn evolved(base: &str, form: &str, power: u8, hp: u8) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base,"state":{"evolved":true,"evolved_with":"form","power":power,"hp":hp,"max_hp":hp}}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":form}]);
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

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
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

fn destroyed_token(printing: &str) -> Game {
    let mut initial = field(printing);
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"execution","card":"BP01-169"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-006"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("execution", PlayerId { seat: 2 })
        .unwrap();
    assert!(
        matches!(finish(&mut game, play), CardPlay::Resolved(_)),
        "Execution resolves before Lastword"
    );
    game.resume_internal_card().unwrap();
    trigger(&mut game, printing);
    game
}

#[test]
fn bp12_012_rush_attack_deals_two_before_combat() {
    let mut initial = field("BP12-012");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-006","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game, "BP12-012");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        3_u8
    );
}

#[test]
fn bp12_015_storm_hits_the_leader_on_entry() {
    let mut initial = field("BP12-015");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u8
    );
}

#[test]
fn bp12_064_evolved_dragoon_heals_only_on_its_own_end() {
    for active in ["P1", "P2"] {
        let mut initial = evolved("BP12-063", "BP12-064", 1, 2);
        initial["turn"]["active"] = json!(active);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        if active == "P1" {
            trigger(&mut game, "BP12-064");
        } else {
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if active == "P1" { 12_u8 } else { 10_u8 },
            "only the controller's end phase heals"
        );
    }
}

#[test]
fn bp12_076_evolves_for_one_pp() {
    evolve_for_one("BP12-076", "BP12-077");
}

#[test]
fn bp12_081_evolved_storm_hits_the_leader() {
    let mut initial = evolved("BP12-080", "BP12-081", 6, 5);
    initial["players"]["P1"]["zones"]["field"][0]["state"]["entered_this_turn"] = json!(true);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        14_u8
    );
}

#[test]
fn bp12_097_ward_protects_its_leader() {
    let mut initial = field("BP12-097");
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
}

#[test]
fn bp12_108_fanfare_acts_the_enemy() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-108"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    trigger(&mut game, "BP12-108");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp12_112_evolves_for_one_pp() {
    evolve_for_one("BP12-112", "BP12-113");
}

#[test]
fn bp12_p26_alias_lastword_heals_after_token_erasure() {
    let game = destroyed_token("BP17-T14");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 14_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert!(state["objects"].get("source").is_none());
}

#[test]
fn bp12_p27_alias_lastword_deals_four_after_token_erasure() {
    let game = destroyed_token("BP17-T15");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
    assert!(state["objects"].get("source").is_none());
}
