#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Lelia scenarios."
)]
use super::damage_prevent::pass_quick;
use super::event_cemetery::finish_checks;
use super::*;

fn position(number: &str, damage: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lelia","card":number,"state":{"damage":damage}}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "lelia",
    )
    .unwrap()
}

fn fire_pending(game: &mut Game) {
    finish_checks(game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed automatic ability")
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        finish_checks(game);
    }
}

#[test]
fn lelia_both_forms_attack_leaders_with_current_health_without_changing_power() {
    for number in ["BP01-129", "BP01-130"] {
        for damage in [0_u8, 2] {
            let mut game = game(&position(number, damage));
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"lelia","target":"P2.leader"}),
            )
            .unwrap();
            pass_quick(&mut game);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P2"]["leader"]["life"],
                14_u8 + damage,
                "combat uses current health, including damage already marked"
            );
            assert_eq!(
                view["objects"]["lelia"]["power"], 0_u8,
                "the rule changes damage rather than the power characteristic"
            );
        }
    }
}

#[test]
fn lelia_applies_to_other_own_followers_only_while_an_effective_provider_is_on_the_field() {
    for status in ["active", "silenced", "cemetery", "two"] {
        let mut initial = position("BP01-129", 0);
        if status == "silenced" {
            initial["players"]["P1"]["zones"]["field"][0]["state"]["silenced"] = json!(true);
        }
        if status == "cemetery" {
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"lelia","card":"BP01-129"}]);
        }
        if status == "two" {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"other","card":"BP01-129"}));
        }
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"mage","card":"BP01-062"}));
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"mage","target":"P2.leader"}))
            .unwrap();
        pass_quick(&mut game);
        let life = if matches!(status, "active" | "two") {
            16_u8
        } else {
            17
        };
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            life,
            "providers are live and do not multiply damage"
        );
    }
}

#[test]
fn lelia_retaliation_uses_health_before_simultaneous_combat_and_does_not_modify_the_enemy() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"mage","card":"BP01-062"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"lelia","card":"BP01-130","state":{"damage":2_u8,"acted":true}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"mage","target":"lelia"}))
        .unwrap();
    pass_quick(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["mage"]),
        "the four-health defender retaliates for four before taking three"
    );
    assert_eq!(
        view["objects"]["lelia"]["hp"], 1_u8,
        "the opposing three-power attacker does not use its four health"
    );
}

#[test]
fn lelia_leaving_during_quick_restores_power_for_the_already_declared_attack() {
    let mut initial = position("BP01-129", 0);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"mage","card":"BP01-062"}));
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-073"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"mage","target":"P2.leader"}))
        .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("defender Quick")
    };
    let CardPlay::Awaiting(targets) = game
        .answer_internal_card(
            quick.actor,
            quick.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap()
    else {
        panic!("removal target")
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("public removal targets")
    };
    let lelia = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-129"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![lelia]))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    pass_quick(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["lelia"]));
    assert_eq!(
        view["P2"]["leader"]["life"], 17_u8,
        "combat checks the provider after the Quick spell removes it"
    );
}

#[test]
fn lelia_does_not_change_jeannes_printed_ability_damage() {
    for number in ["BP01-129", "BP01-130"] {
        let mut initial = position(number, 0);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"jeanne","card":"BP01-127"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"mage","card":"BP01-062"},{"id":"goblin","card":"BP01-171"}]);
        let mut game = game(&initial);
        assert!(
            matches!(
                game.play_internal_card("jeanne", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "Jeanne enters before its separately produced fanfare"
        );
        fire_pending(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["mage"]["hp"], 2_u8,
            "Q226/Q227 retain the printed two ability damage"
        );
        assert_eq!(view["P2"]["cemetery"], json!(["goblin"]));
    }
}

#[test]
fn lelia_evolves_for_one_pp_and_gains_health_at_its_own_end_even_when_uninjured() {
    for damage in [0_u8, 2] {
        let mut initial = position("BP01-129", damage);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-130"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"lelia","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the printed evolution cost is one PP"
        );
        fire_pending(&mut game);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        fire_pending(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["lelia"]["max_hp"], 8_u8,
            "Q228 increases maximum health rather than only healing damage"
        );
        assert_eq!(view["objects"]["lelia"]["hp"], 8_u8 - damage);
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    }
}
