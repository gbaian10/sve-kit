#![expect(
    clippy::panic,
    reason = "Unexpected questions cannot complete a real-card scenario."
)]
use super::*;

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only entry posture, formal targets and optional payment are accepted in these scenarios."
)]
fn settle(game: &mut Game, mut play: CardPlay) {
    loop {
        match play {
            CardPlay::Awaiting(question) => {
                let answer = match &question.domain {
                    InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                    InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                    InputDomain::OptionalCost => Answer::OptionalCost(true),
                    _ => panic!("unexpected entry question"),
                };
                play = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
            CardPlay::Entered(_) | CardPlay::Resolved(_) => break,
            _ => panic!("entry failed"),
        }
    }
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("entry pending");
    };
    assert_eq!(
        options.len(),
        1,
        "only the real card's matching trigger runs"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    settle(game, play);
}

fn position(card: &str, from: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][from] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn entered(card: &str, ability: bool) -> Game {
    if ability {
        let initial = position(card, "cemetery");
        let mut game = with_producer(
            &initial,
            Action::Move {
                objects: subjects_in(ZoneKind::Cemetery),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Field,
                        },
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            },
        );
        let play = game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        settle(&mut game, play);
        game.resume_internal_card().unwrap();
        game
    } else {
        let mut game = game(&position(card, "hand"));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        settle(&mut game, play);
        game
    }
}

#[test]
fn ability_entry_bp06_008_grants_storm_only_after_ability_put() {
    for ability in [false, true] {
        let mut game = entered("BP06-008", ability);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            ability,
            "the entry method controls Storm"
        );
    }
}

#[test]
fn ability_entry_bp06_009_evolution_damages_its_formal_target() {
    let mut initial = position("BP06-008", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-009"}]);
    let mut game = game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8,
        "the evolved real card deals two damage"
    );
}

#[test]
fn ability_entry_bp17_095_draws_only_after_ability_put() {
    for ability in [false, true] {
        let mut game = entered("BP17-095", ability);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            if ability { json!(["drawn"]) } else { json!([]) },
            "only ability entry draws"
        );
    }
}

#[test]
fn ability_entry_bp21_033_gains_power_and_assail_only_after_ability_put() {
    for ability in [false, true] {
        let mut game = entered("BP21-033", ability);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if ability { 4_u8 } else { 2_u8 },
            "the permanent buff depends on entry method"
        );
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃")),
            ability,
            "Assail shares the condition"
        );
    }
}

#[test]
fn ability_entry_bp21_091_selects_a_target_but_damages_only_after_ability_put() {
    for ability in [false, true] {
        let mut game = entered("BP21-091", ability);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if ability { 2_u8 } else { 0_u8 },
            "normal play still selects but does not damage"
        );
    }
}
