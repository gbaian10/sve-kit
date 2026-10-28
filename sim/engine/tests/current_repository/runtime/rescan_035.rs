#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "rescan-035",
    )
    .unwrap()
}

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

fn assail(card: &str) {
    let mut control = field_position("BP01-037");
    control["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut control_game = game(&control);
    assert_eq!(
        control_game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "ordinary followers cannot attack active followers"
    );
    let mut initial = field_position(card);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "assail permits attacking an active follower"
    );
}

#[test]
fn cp03_076_rush_assail_and_awakening_uses_maximum_pp() {
    for print in ["CP03-076", "CP03-P53", "CSD03b-012"] {
        immediate_attack(print, "rush", 3);
        assail(print);
        for max in [6_u8, 7] {
            let mut initial = position(print);
            initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":max});
            let mut game = game(&initial);
            play(&mut game);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if max == 7 { 18_u8 } else { 20_u8 },
                "awakening uses maximum PP even after current PP is spent"
            );
            assert_eq!(state["P1"]["leader"]["life"], 20_u8);
            assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        }
    }
}

#[test]
fn cp03_075_quick_deals_six_on_the_opponents_turn() {
    for print in ["CP03-075", "CP03-P52"] {
        let mut initial = position(print);
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        let quick = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &quick.domain else {
            panic!("expected Quick");
        };
        assert_eq!(options.len(), 1);
        let result = game
            .answer_internal_card(quick.actor, quick.token, Answer::Quick(Some(0)))
            .unwrap();
        finish(&mut game, result, true);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert_eq!(state["P1"]["leader"]["life"], 20_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn cp03_038_ward_and_fanfare_heal_only_the_controller() {
    for print in ["CP03-038", "CP03-P26", "CSD03a-015"] {
        ward(print);
        let mut game = game(&position(print));
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 22_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn cp03_037_stands_only_at_its_own_end_phase() {
    for print in ["CP03-037", "CP03-P25", "CSD03a-014"] {
        for active in ["P1", "P2"] {
            let mut initial = field_position(print);
            initial["turn"]["active"] = json!(active);
            initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
            if active == "P1" {
                trigger(&mut game);
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                active != "P1"
            );
        }
    }
}

#[test]
fn cp03_080_act_cost_buffs_power_once_before_standing_again() {
    for print in ["CP03-080", "CP03-P57"] {
        let mut game = game(&field_position(print));
        let attempt = game
            .activate_internal_card("source", AbilityId(1), actor())
            .unwrap();
        finish(&mut game, attempt, true);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert_eq!(state["objects"]["source"]["hp"], 2_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
        assert!(
            game.activate_internal_card("source", AbilityId(1), actor())
                .is_err(),
            "an acted follower cannot pay the act cost again"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            2_u8
        );
    }
}

#[test]
fn cp03_093_fanfare_mills_only_its_controllers_top_card() {
    for print in ["CP03-093", "CP03-P62"] {
        let mut game = game(&position(print));
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["P1-first"]));
        assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
        assert_eq!(
            state["P2"]["deck"],
            json!(["P2-first", "P2-second", "P2-third"])
        );
    }
}
