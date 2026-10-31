#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the UB scenarios."
)]
use super::*;

fn position(card: &str, zone: &str, construction: &str, title: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!(construction);
    initial["players"]["P1"]["title_code"] = json!(title);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn from_position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ub",
    )
    .unwrap()
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn target(game: &mut Game, play: CardPlay) -> Activation {
    let CardPlay::Awaiting(question) = play else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal target")
    };
    let enemy = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![enemy.handle]),
        )
        .unwrap(),
    )
}

fn qualified(activation: &Activation) {
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                Fact::AbilityPlayed {
                    qualification: AbilityQualification::UnionBurst,
                    ..
                }
            ))
            .count(),
        1,
        "one proper UB play regardless of body facts"
    );
}

#[test]
fn suzume_printings_heal_only_in_the_bound_title_construction() {
    for card in ["CP04-105", "CP04-P81"] {
        for (construction, title, enabled) in [
            ("title", "princess_connect_re_dive", true),
            ("class", "princess_connect_re_dive", false),
            ("crossover", "princess_connect_re_dive", false),
            ("title", "umamusume_pretty_derby", false),
        ] {
            let initial = position(card, "hand", construction, title);
            let mut game = from_position(&initial);
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            if enabled {
                qualified(&resolved(trigger(&mut game, card)));
            } else {
                while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                    game.resume_internal_card().unwrap();
                }
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if enabled { 12_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn aoi_printings_pay_posture_and_reduce_the_declared_enemy_stats() {
    for card in ["CP04-015", "CP04-P11"] {
        let initial = position(card, "field", "title", "princess_connect_re_dive");
        let mut game = from_position(&initial);
        let before = game.projection(View::Referee).unwrap();
        let definition = selection_with_rules().current[card]
            .program
            .card()
            .abilities[0]
            .node
            .id;
        let play = game
            .activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap();
        qualified(&target(&mut game, play));
        let after = game.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["enemy"][stat].as_i64().unwrap(),
                before["objects"]["enemy"][stat].as_i64().unwrap() - 1
            );
        }
        assert_eq!(after["objects"]["source"]["acted"], true);
        let mut initial = initial;
        initial["players"]["P1"]["construction"] = json!("class");
        let mut disabled = from_position(&initial);
        let disabled_before = disabled.projection(View::Referee).unwrap();
        disabled
            .activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(disabled.projection(View::Referee).unwrap(), disabled_before);
    }
}

#[test]
fn misaki_attack_uses_the_same_ub_gate_and_cancels_without_a_target() {
    for card in ["CP04-086", "CP04-P66"] {
        for available in [false, true] {
            let mut initial = position(card, "field", "title", "princess_connect_re_dive");
            if !available {
                initial["players"]["P2"]["zones"]["field"] = json!([]);
            }
            let mut game = from_position(&initial);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            let play = trigger(&mut game, card);
            if available {
                qualified(&target(&mut game, play));
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                    3_u8
                );
            } else {
                assert_eq!(play, CardPlay::Cancelled);
            }
        }
    }
}
