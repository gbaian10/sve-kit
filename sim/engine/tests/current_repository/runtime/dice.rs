#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed dice scenarios."
)]
use super::*;

fn printings(number: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[number].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(printing, _)| printing.clone())
        .collect()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"}]);
    initial
}

fn seeded(initial: &Value, seed: &str) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        seed,
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> Vec<DieFace> {
    let mut rolls = vec![];
    loop {
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(1)
                        .map(|member| member.handle)
                        .collect(),
                ),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                other => panic!("unexpected dice decision: {other:?}"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        if let CardPlay::Resolved(done) = &play {
            rolls.extend(done.progress.facts.iter().filter_map(|fact| match fact {
                Fact::Dice(event) => Some(event.value),
                _ => None,
            }));
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return rolls;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending dice ability");
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
}

fn activate(game: &mut Game, holder: &str) -> Vec<DieFace> {
    let ability = game.internal_activated_sources(holder).unwrap()[0].definition;
    let play = game
        .activate_internal_card(holder, ability, PlayerId { seat: 1 })
        .unwrap();
    finish(game, play)
}

#[test]
fn bp21_083_accepted_six_damages_the_opponent_and_the_activation_is_once_per_turn() {
    for number in printings("BP21-083") {
        for (seed, face, life) in [
            ("dice-3", DieFace::Six, 18_u8),
            ("dice-0", DieFace::Three, 20_u8),
        ] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            let mut game = seeded(&initial, seed);
            assert_eq!(activate(&mut game, "source"), vec![face]);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                life
            );
            let before = format!("{game:?}");
            let source = game.internal_activated_sources("source").unwrap()[0];
            game.activate_internal_card("source", source.definition, PlayerId { seat: 1 })
                .unwrap_err();
            assert_eq!(format!("{game:?}"), before);
        }
    }
}

#[test]
fn bp21_081_checks_current_and_earlier_sixes_in_the_same_turn() {
    for number in printings("BP21-081") {
        for (prior, seed, face, evolved) in [
            (true, "dice-3", DieFace::Four, true),
            (false, "dice-3", DieFace::Six, true),
            (false, "dice-0", DieFace::Three, false),
        ] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"roller","card":"BP21-083"}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"maron","card":number}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP21-082"}]);
            let mut game = seeded(&initial, seed);
            if prior {
                assert_eq!(activate(&mut game, "roller"), vec![DieFace::Six]);
            }
            let play = game
                .play_internal_card("maron", PlayerId { seat: 1 })
                .unwrap();
            assert_eq!(finish(&mut game, play), vec![face]);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["maron"]["evolved"], evolved);
            assert_eq!(
                state["P1"]["evolve_deck"],
                if evolved { json!([]) } else { json!(["form"]) }
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 9_u8,
                "effect evolution does not pay the printed intrinsic cost"
            );
        }
    }
}

#[test]
fn bp21_080_draws_then_rolls_twice_and_each_final_result_triggers() {
    for number in printings("BP21-080") {
        for (seed, expected) in [
            ("dice-3", vec![DieFace::Six, DieFace::Four]),
            ("dice-9", vec![DieFace::Five, DieFace::Six]),
        ] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"observer","card":"BP21-083"}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
            let mut game = seeded(&initial, seed);
            let play = game
                .play_internal_card("spell", PlayerId { seat: 1 })
                .unwrap();
            assert_eq!(finish(&mut game, play), expected);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["d1"]));
            assert_eq!(
                state["P2"]["leader"]["life"], 18_u8,
                "each pending ability retains its own roll value"
            );
        }
    }
}
