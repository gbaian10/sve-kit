#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these change-event scenarios."
)]
use super::event_cemetery::{finish_checks, move_named, position, prepared};
use super::*;

fn subjects(name: &str) -> Selector {
    let Action::Move { objects, .. } = move_named(ZoneKind::Field, OwnerZone::Hand, name) else {
        panic!("named subject")
    };
    objects
}
fn modification(name: &str, facets: &[(Stat, i64)]) -> Action {
    Action::Modify {
        subjects: subjects(name),
        patch: CharacteristicPatch::Combined {
            patches: facets
                .iter()
                .map(|(stat, n)| CharacteristicPatch::Numeric {
                    stat: *stat,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(*n)),
                    },
                })
                .collect(),
        },
        expires: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        source: EffectReference::CurrentInstallation {},
    }
}
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("printed target or Ward posture");
            };
            Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("movement-enemy"))
                    .unwrap()
                    .handle,
            ])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed ability resolves"
    );
    finish_checks(game);
}
fn occurrence(game: &mut Game, card: &str, expected: bool) {
    let pending = game.internal_pending_choice().unwrap();
    if !expected {
        assert!(
            pending.is_none(),
            "a nonmatching change creates no occurrence"
        );
        return;
    }
    let pending = pending.unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("change occurrence")
    };
    assert_eq!(
        options.len(),
        1,
        "one commit is one occurrence even when both stats increase"
    );
    assert_eq!(
        options[0].card, card,
        "the printed observer owns this occurrence"
    );
    let play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

#[test]
fn outlaw_printings_gain_storm_from_either_stat_once_per_commit() {
    for card in ["BP11-113", "BP11-P27"] {
        for (facets, expected) in [
            (vec![(Stat::Attack, 1)], true),
            (vec![(Stat::Life, 1)], true),
            (vec![(Stat::Attack, 1), (Stat::Life, 1)], true),
            (vec![(Stat::Attack, 0)], false),
            (vec![(Stat::Attack, -1)], false),
        ] {
            let mut initial = position(card, "P1", "movement-unit", "P1", "field");
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
            initial["players"]["P1"]["zones"]["field"][0]["state"] =
                json!({"entered_this_turn":true});
            let mut game = prepared(&initial, modification("レヴィールの無法者", &facets));
            activate(&mut game, "P1");
            finish_checks(&mut game);
            occurrence(&mut game, card, expected);
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome
                    == "resolved",
                expected
            );
        }
    }
}

#[test]
fn holy_cavalry_printings_gain_rush_only_from_attack_increases() {
    for card in ["BP21-P68", "BP21-T08"] {
        for (stat, expected) in [(Stat::Attack, true), (Stat::Life, false)] {
            let mut initial = position(card, "P1", "movement-unit", "P1", "field");
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
            initial["players"]["P1"]["zones"]["field"][0]["state"] =
                json!({"entered_this_turn":true});
            let mut game = prepared(&initial, modification("聖騎兵", &[(stat, 1)]));
            activate(&mut game, "P1");
            finish_checks(&mut game);
            occurrence(&mut game, card, expected);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome
                    == "resolved",
                expected
            );
        }
    }
}

#[test]
fn amaterasu_printings_heal_only_on_actual_act_and_use_the_resolution_board() {
    for card in ["BP09-108", "BP09-P31"] {
        for (acted, paired, expected) in [(false, false, 1_u8), (false, true, 2), (true, false, 0)]
        {
            let mut initial = position(card, "P1", "movement-unit", "P1", "field");
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":acted});
            if paired {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"moon","card":"BP09-109"}));
            }
            let mut game = prepared(
                &initial,
                Action::Posture {
                    objects: subjects("アマテラス"),
                    posture: Posture::Acted,
                },
            );
            activate(&mut game, "P1");
            finish_checks(&mut game);
            occurrence(&mut game, card, expected > 0);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10_u8 + expected
            );
        }
    }
}

#[test]
fn tsukuyomi_printings_damage_leaders_from_attacking_and_do_not_repeat_an_already_acted_change() {
    for card in ["BP09-109", "BP09-P32"] {
        let mut initial = position(card, "P1", "movement-unit", "P1", "field");
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = prepared(
            &initial,
            Action::Posture {
                objects: subjects("ツクヨミ"),
                posture: Posture::Acted,
            },
        );
        let step = game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        occurrence(&mut game, card, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            19_u8
        );
    }
}

#[test]
fn capture_string_printings_choose_damage_after_act() {
    for card in ["CP04-102", "CP04-P78", "PCS01-044"] {
        let mut game = prepared(
            &position(card, "P1", "movement-unit", "P1", "field"),
            Action::Posture {
                objects: subjects("キャプチャーストリング"),
                posture: Posture::Acted,
            },
        );
        activate(&mut game, "P1");
        finish_checks(&mut game);
        occurrence(&mut game, card, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            1_u8
        );
    }
}

#[test]
fn wilbert_printings_create_the_holy_cavalry_with_its_stat_observer() {
    for card in ["BP21-094", "BP21-SL24"] {
        let mut initial = position(card, "P1", "movement-unit", "P1", "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        let mut game = prepared(&initial, modification("聖騎兵", &[(Stat::Attack, 1)]));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        occurrence(&mut game, card, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 4);
        activate(&mut game, "P1");
        finish_checks(&mut game);
        occurrence(&mut game, "BP21-P68", true);
    }
}

fn resolve_line(game: &mut Game, card: &str, line: u64, count: usize) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed evolution or stat occurrence")
    };
    assert_eq!(
        options.len(),
        count,
        "ordinary and super evolution collect the expected facts"
    );
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    finish(game, play);
}

#[test]
fn evolved_outlaw_inherits_base_values_without_a_stat_fact_but_super_evolution_adds_one() {
    for (base, form) in [("BP11-113", "BP11-114"), ("BP11-P27", "BP11-P28")] {
        for (super_evolved, field_vehicles, ex_vehicles) in
            [false, true].into_iter().flat_map(|super_evolved| {
                [(0_u8, 0_u8), (1, 0), (0, 1), (2, 0), (0, 2), (1, 1)]
                    .into_iter()
                    .map(move |(field, ex)| (super_evolved, field, ex))
            })
        {
            let mut initial = position(base, "P1", "movement-other", "P1", "field");
            initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":7_u8});
            initial["players"]["P1"]["sep"] = json!(1_u8);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            if field_vehicles > 0 {
                initial["players"]["P1"]["zones"]["field"][1]["card"] = json!("movement-unit");
            }
            if field_vehicles > 1 {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"second-vehicle","card":"movement-unit"}));
            }
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..ex_vehicles)
                    .map(|index| json!({"id":format!("vehicle-{index}"),"card":"movement-amulet"}))
                    .collect::<Vec<_>>()
            );
            let mut game = prepared(
                &initial,
                modification("レヴィールの無法者", &[(Stat::Attack, 0)]),
            );
            let step = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":u8::from(super_evolved)}})).unwrap();
            assert_eq!(step.outcome, "resolved");
            if super_evolved {
                resolve_line(&mut game, form, 1, 2);
            }
            resolve_line(&mut game, form, 2, 1);
            assert!(game.internal_pending_choice().unwrap().is_none());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if field_vehicles + ex_vehicles >= 2 {
                    2_u8
                } else {
                    0_u8
                }
            );
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                super_evolved
            );
        }
    }
}

#[test]
fn amaterasu_heals_when_ward_acts_after_entry() {
    let mut initial = position("BP09-108", "P1", "movement-unit", "P1", "field");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-108"}]);
    let mut game = prepared(&initial, modification("アマテラス", &[(Stat::Attack, 0)]));
    let CardPlay::Awaiting(question) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Ward entry posture")
    };
    assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::WardEntry(true))
        .unwrap();
    finish(&mut game, play);
    occurrence(&mut game, "BP09-108", true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn amaterasu_heals_when_end_phase_ward_acts() {
    let mut initial = position("BP09-108", "P1", "movement-unit", "P1", "field");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = prepared(&initial, modification("アマテラス", &[(Stat::Attack, 0)]));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.internal_rule_action(&json!({"do":"guard-act","select":["source"]}))
            .unwrap()
            .outcome,
        "resolved"
    );
    occurrence(&mut game, "BP09-108", true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        11_u8
    );
}
