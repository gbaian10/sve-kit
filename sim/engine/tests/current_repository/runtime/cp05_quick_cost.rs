#![expect(
    clippy::panic,
    reason = "A different decision invalidates the printed-card scenario."
)]

use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

#[test]
fn folia_makes_the_next_quick_activation_affordable_and_consumes_the_reduction() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":4_u8});
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"folia","card":"CP05-047"},{"id":"hakutaku","card":"CP05-T07"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"other","card":"CP05-039"},{"id":"spell","card":"BP01-098"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let first = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick {
        options: initial_options,
        ..
    } = &first.domain
    else {
        panic!("expected Quick choice");
    };
    assert_eq!(initial_options.len(), 1);
    assert_eq!(initial_options[0].card, "CP05-047");
    resolved(
        game.answer_internal_card(
            first.actor,
            first.token,
            Answer::Quick(Some(initial_options[0].handle)),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();

    let after_folia = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &after_folia.domain else {
        panic!("expected Quick choice");
    };
    assert_eq!(options.len(), 2);
    let hakutaku = options
        .iter()
        .find(|option| option.card == "CP05-T07")
        .unwrap();
    resolved(
        game.answer_internal_card(
            after_folia.actor,
            after_folia.token,
            Answer::Quick(Some(hakutaku.handle)),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();

    let after_hakutaku = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(&after_hakutaku.domain, InputDomain::Quick { options: remaining, .. } if remaining.is_empty())
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        17_u8
    );
}

fn resolve_pending(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending trigger");
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn super_evolved_wamdus_reduces_the_three_printed_quick_costs() {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP05-039"},{"id":"hakutaku","card":"CP05-T07"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell","card":"BP01-065"},{"id":"follower","card":"CP05-043"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-040"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("draw", "BP01-037", 4));
    initial["players"]["P2"]["pp"] = json!({"current":1_u8,"max":1_u8});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"opponent_hakutaku","card":"CP05-T07"}]);
    let mut game = game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}}),
    )
    .unwrap();
    resolve_pending(&mut game, "CP05-040");
    resolved(game.play_internal_card("spell", PLAYER).unwrap());
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8
    );

    assert!(matches!(
        game.play_internal_card("follower", PLAYER).unwrap(),
        CardPlay::Entered(_)
    ));
    resolve_pending(&mut game, "CP05-043");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8
    );

    resolved(
        game.activate_internal_card(
            "hakutaku",
            definition("CP05-T07", AbilityKind::Activated, 0),
            PLAYER,
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8
    );

    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    resolve_pending(&mut game, "CP05-040");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8,
        "Wamdus restores three PP at its controller's end phase",
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap()
            .outcome,
        "resolved",
        "the active player declines to act any Guard followers",
    );
    let choice = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        choice.actor,
        PlayerId { seat: 2 },
        "the opponent has the Quick window"
    );
    let InputDomain::Quick { options, .. } = &choice.domain else {
        panic!("expected the opponent's Quick choice");
    };
    let option = options
        .iter()
        .find(|option| option.card == "CP05-T07")
        .unwrap();
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Quick(Some(option.handle)),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
        0_u8,
        "the opponent pays the printed point despite Wamdus",
    );
}

#[test]
fn wamdus_reduces_dragonic_rage_only_while_the_caster_is_awakened() {
    for (zone, max_pp, remaining_pp) in [("hand", 6_u8, 0_u8), ("hand", 7, 1), ("ex", 7, 1)] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":max_pp});
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-039"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-040"}]);
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"spell","card":"BP12-058"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":8_u8,"max_hp":8_u8}}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({
                "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}
            }))
            .unwrap()
            .outcome,
            "resolved"
        );
        resolve_pending(&mut game, "CP05-040");
        let play = game.play_internal_card("spell", PLAYER).unwrap();
        resolved(choose_card_by_number(&mut game, play, "BP01-037"));
        let board = game.projection(View::Referee).unwrap();
        assert_eq!(
            board["P1"]["pp"]["current"], remaining_pp,
            "PP cap {max_pp}, played from {zone}"
        );
        assert_eq!(board["P1"]["cemetery"], json!(["spell"]));
        assert_eq!(board["objects"]["enemy"]["hp"], 4_u8);
    }
}
