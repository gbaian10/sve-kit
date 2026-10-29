#![expect(
    clippy::panic,
    reason = "Unexpected authority questions invalidate these real printed-card paths."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(base: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":base}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "real-cemetery-play",
    )
    .unwrap()
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("real base entry")
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("real trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "only this printed source contributes a trigger"
    );
    assert_eq!(
        options[0].card, card,
        "the real printing supplies the pending ability"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn bp04_064_aliases_wait_until_the_next_real_dragon_spell_then_pay_one_to_return() {
    for base in ["BP04-064", "BP04-P16", "PR-084"] {
        let mut initial = position(base);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":base},
            {"id":"first","card":"BP01-099"},
            {"id":"second","card":"BP01-099"}
        ]);
        let mut game = game(&initial);
        enter(&mut game);
        let entered = game.projection(View::Referee).unwrap();
        assert_eq!(entered["P1"]["pp"]["current"], 7_u8);
        let generation = entered["objects"]["source"]["generation"].as_u64().unwrap();
        resolved(game.play_internal_card("first", actor()).unwrap());
        game.resume_internal_card().unwrap();
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "a source on the field when the Spell was played cannot trigger after that Spell kills it"
        );
        let dead = game.projection(View::Referee).unwrap();
        assert_eq!(dead["P1"]["field"], json!([]));
        assert!(
            dead["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert_eq!(dead["P1"]["pp"]["current"], 4_u8);
        resolved(game.play_internal_card("second", actor()).unwrap());
        game.resume_internal_card().unwrap();
        let CardPlay::Awaiting(payment) = trigger(&mut game, base) else {
            panic!("printed one PP declaration")
        };
        assert!(matches!(payment.domain, InputDomain::OptionalCost));
        let returned = resolved(
            game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P1"]["cemetery"], json!(["first", "second"]));
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["objects"]["source"]["card"], base);
        assert_eq!(state["objects"]["source"]["generation"], generation + 2);
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert_eq!(state["objects"]["source"]["hp"], 2_u8);
        assert_eq!(state["objects"]["source"]["damage"], 0_u8);
        assert!(returned.progress.facts.iter().any(|fact|matches!(fact,Fact::Movement(member) if member.from.kind==ZoneKind::Cemetery && member.movement.actual_to==Some(RuntimeZoneRef {player:actor(),kind:ZoneKind::Field}))));
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn bp04_065_aliases_evolve_from_the_real_base_and_deal_two_before_combat() {
    for (base, form) in [
        ("BP04-064", "BP04-065"),
        ("BP04-P16", "BP04-P17"),
        ("PR-084", "PR-085"),
    ] {
        let mut initial = position(base);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] =
            json!({"damage":3_u8,"acted":true});
        let mut game = game(&initial);
        enter(&mut game);
        let evolved = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(evolved.outcome, "resolved");
        assert!(evolved.events.iter().any(|event| event["kind"] == "進化"
            && event["object"] == "source"
            && event["with"] == "form"));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        let CardPlay::Awaiting(question) = trigger(&mut game, form) else {
            panic!("real OnAttack target")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal target")
        };
        assert_eq!(domain.members.len(), 1);
        let damage = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        assert!(
            damage
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact,Fact::Damage(event) if event.amount.0.get()==2))
        );
        game.resume_internal_card().unwrap();
        if let Some(quick) = game.internal_quick_choice().unwrap() {
            assert!(matches!(
                game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                    .unwrap(),
                CardPlay::Continued(_)
            ));
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P1"]["evolve_deck"], json!([]));
        assert_eq!(state["objects"]["form"]["card"], form);
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert_eq!(
            state["objects"]["source"]["damage"], 0_u8,
            "the target left before combat could hit back"
        );
    }
}
