#![expect(
    clippy::panic,
    reason = "A different pending domain or decision invalidates these scenarios."
)]
use super::*;
use sve_engine::game::internal::{InstructionOutcome, ResourceOutcome};

fn resolve_maximum(game: &mut Game, seat: &str, maximum: i64) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the departed follower's Lastword");
    };
    assert_eq!(
        options.len(),
        1,
        "only the departed follower has a pending ability"
    );
    assert_eq!(
        options[0].card, "BP01-079",
        "the original printed source is retained"
    );
    assert_eq!(options[0].line, 2_u64, "only the Lastword is pending");
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: if question.actor.seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "wrong actor must not alter the pending ability"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let actual = i64::from(maximum != 10);
    assert_eq!(
        result.source.controller.seat,
        seat_number(seat),
        "Lastword keeps the original controller"
    );
    assert_eq!(
        result.progress.receipts.len(),
        1,
        "one resource instruction completes"
    );
    assert_eq!(
        result.progress.receipts.values().next().unwrap(),
        &InstructionOutcome::Resource(ResourceOutcome {
            player: question.actor,
            resource: Resource::PpMaximum,
            requested: Int(1),
            before: Int(maximum),
            after: Int(maximum.saturating_add(actual)),
            actual: Int(actual),
        }),
        "the receipt distinguishes the printed request and the capped increase"
    );
    assert!(
        result.progress.facts.iter().all(|fact| matches!(
            fact,
            Fact::AbilityPlayed { .. } | Fact::AbilityResolved { .. }
        )),
        "the resource body adds no facts beyond the existing ability lifecycle"
    );
    let completed = format!("{game:?}");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == completed,
        "old pending token must not resolve twice"
    );
}

#[test]
fn bp01_079_lastword_increases_original_controllers_cap_without_recovering_pp() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for maximum in [3_i64, 9, 10] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"][opponent]["pp"] = json!({"current":1_u8,"max":4_u8});
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"aira","card":"BP01-079","owner":opponent}]);
            let mut game = with_producer(
                &initial,
                Action::Destroy {
                    objects: subjects(),
                },
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
            activate(&mut game, seat);
            game.resume_internal_card().unwrap();
            resolve_maximum(&mut game, seat, maximum);
            for view in [View::P1, View::P2, View::Referee] {
                let state = game.projection(view).unwrap();
                assert_eq!(
                    state[seat]["pp"],
                    json!({"current":maximum,"max":maximum.saturating_add(1).min(10)})
                );
                assert_eq!(state[opponent]["pp"], json!({"current":1_u8,"max":4_u8}));
                assert!(
                    state[opponent]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("aira"))
                );
            }
        }
    }
}

#[test]
fn bp01_079_banish_does_not_trigger_a_maximum_pp_increase() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["pp"] = json!({"current":2_u8,"max":3_u8});
        initial["players"][seat]["zones"]["field"] = json!([{"id":"aira","card":"BP01-079"}]);
        let mut game = with_producer(
            &initial,
            Action::Banish {
                objects: subjects(),
            },
        );
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["pp"], json!({"current":2_u8,"max":3_u8}));
        assert!(
            state[seat]["banish"]
                .as_array()
                .unwrap()
                .contains(&json!("aira"))
        );
    }
}

#[test]
fn bp01_079_keeps_assail_against_an_active_enemy_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"aira","card":"BP01-079"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":false}}]);
    let mut game = game(&initial);
    let attack = game
        .internal_rule_action(&json!({"do":"attack","attacker":"aira","target":"enemy"}))
        .unwrap();
    assert_eq!(attack.outcome, "resolved");
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
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_) | CardPlay::Entered(_) | CardPlay::Continued(_)
        ),
        "the effect completes"
    );
}

fn targeted_evolution(base: &str, form: &str, damage: u8, after_super: [u8; 3]) {
    for (super_evolved, expected) in [(false, [15, 7, 1]), (true, after_super)] {
        let mut initial = field_position(base);
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":7_u8});
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut pay = json!({"pp":1_u8});
        if super_evolved {
            pay["sep"] = json!(1_u8);
        }
        let mut game = game(&initial);
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
            )
            .unwrap();
        assert_eq!(
            step.outcome, "resolved",
            "the printed cost evolves the real base"
        );
        for remaining in (1..=if super_evolved { 2 } else { 1 }).rev() {
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("expected an evolution trigger");
            };
            assert_eq!(
                options.len(),
                remaining,
                "super evolution triggers both printed heads"
            );
            let result = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            finish(&mut game, result, true);
            game.resume_internal_card().unwrap();
        }
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "all evolution triggers have resolved"
        );
        let state = game.projection(View::Referee).unwrap();
        let resources = json!([
            state["P1"]["leader"]["life"],
            state["P1"]["pp"]["max"],
            state["P1"]["pp"]["current"]
        ]);
        assert_eq!(
            resources,
            json!(expected),
            "printed life and PP after payment"
        );
        assert_eq!(
            state["P1"]["sep"],
            u8::from(!super_evolved),
            "only super evolution spends SEP"
        );
        assert_eq!(
            state["objects"]["source"]["evolved_with"], "form",
            "the real base retains its resource"
        );
        assert_eq!(
            state["P1"]["evolution"],
            json!(["form"]),
            "the resource enters the evolution zone"
        );
        if damage == 5 {
            assert_eq!(
                state["P2"]["field"],
                json!([]),
                "lethal damage destroys the selected follower"
            );
            assert_eq!(
                state["P2"]["cemetery"],
                json!(["enemy"]),
                "the defeated follower enters its cemetery"
            );
        } else {
            assert_eq!(
                state["P2"]["field"],
                json!(["enemy"]),
                "three damage leaves the five-life follower alive"
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"], damage,
                "the formal target receives printed damage"
            );
        }
    }
}

#[test]
fn bp16_062_real_evolution_targets_and_super_evolution_increases_maximum_pp() {
    targeted_evolution("BP16-061", "BP16-062", 3, [17, 8, 1]);
    targeted_evolution("BP16-061", "SD08-008", 3, [17, 8, 1]);
}

#[test]
fn bp02_057_spell_adds_max_pp_heals_and_draws() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["pp"] = json!({"current":9_u8,"max":9_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP02-057"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);

    let result = resolved(game.play_internal_card("spell", actor()).unwrap());
    game.resume_internal_card().unwrap();

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["max"], 10_u8,
        "the spell raises maximum PP by one"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 4_u8,
        "increasing maximum PP does not restore the five PP paid for the spell"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 13_u8,
        "the spell restores three life"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["drawn"]),
        "the spell draws one card"
    );
    assert!(
        !result.progress.receipts.is_empty(),
        "the spell records completed effects"
    );
}
