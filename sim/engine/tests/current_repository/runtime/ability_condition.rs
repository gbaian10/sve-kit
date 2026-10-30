#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed scenarios."
)]
use super::*;
use std::sync::OnceLock;

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal target")
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
}

fn initial(card: &str, seat: &str) -> Value {
    let mut value = draw_position();
    value["turn"]["active"] = json!(seat);
    value["players"][seat]["zones"]["field"] = json!([{"id":"source","card":card}]);
    let other = if seat == "P1" { "P2" } else { "P1" };
    value["players"][other]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    value
}

fn ability(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn bp09_065_requires_three_allied_field_dragon_warriors_before_any_choice_or_payment() {
    for card in ["BP09-065", "DSD01b-017", "PR-228"] {
        for seat in ["P1", "P2"] {
            let other = if seat == "P1" { "P2" } else { "P1" };
            for (extra, owner, zone, allowed) in [
                (1_usize, seat, "field", false),
                (2, seat, "field", true),
                (2, other, "field", false),
                (2, seat, "cemetery", false),
            ] {
                let mut value = initial(card, seat);
                let material = value["players"][owner]["zones"][zone].as_array_mut();
                let cards = (0..extra).map(|i| json!({"id":format!("ally-{i}"),"card":"BP09-065"}));
                if let Some(material) = material {
                    material.extend(cards);
                } else {
                    value["players"][owner]["zones"][zone] = json!(cards.collect::<Vec<_>>());
                }
                let mut game = game(&value);
                let before = format!("{game:?}");
                let result = game.activate_internal_card(
                    "source",
                    ability(card),
                    PlayerId {
                        seat: seat_number(seat),
                    },
                );
                if allowed {
                    finish(&mut game, result.unwrap());
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        state["objects"]["enemy"]["damage"], 3_u8,
                        "the printed ability deals three"
                    );
                    assert_eq!(
                        state["objects"]["source"]["acted"], true,
                        "the legal use pays act"
                    );
                } else {
                    assert!(
                        matches!(result, Err(EngineFailure::IllegalPlay(_))),
                        "the threshold prevents use"
                    );
                    assert!(
                        format!("{game:?}") == before,
                        "no target question, cost or source instance survives refusal"
                    );
                }
            }
        }
    }
}

#[test]
fn bp16_090_requires_ten_cards_in_its_controllers_cemetery() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        for (count, owner, allowed) in [(9, seat, false), (10, seat, true), (10, other, false)] {
            let mut value = initial("BP16-090", seat);
            value["players"][owner]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-037"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&value);
            let before = format!("{game:?}");
            let result = game.activate_internal_card(
                "source",
                ability("BP16-090"),
                PlayerId {
                    seat: seat_number(seat),
                },
            );
            if allowed {
                finish(&mut game, result.unwrap());
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["objects"]["enemy"]["damage"], 3_u8,
                    "ten cards unlock the printed damage"
                );
                assert_eq!(
                    state[seat]["cemetery"].as_array().unwrap().len(),
                    count,
                    "the gate does not consume the cemetery"
                );
            } else {
                assert!(
                    matches!(result, Err(EngineFailure::IllegalPlay(_))),
                    "only the allied threshold permits use"
                );
                assert!(format!("{game:?}") == before, "failed use is atomic");
            }
        }
    }
}

fn with_form(value: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut face = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|entry| entry["number"] == "BP16-060").unwrap()["faces"][0].clone();
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        snapshot.push_str(&json!({"number":"condition-form","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["condition-form.yaml",
            "format: 1\nkind: effect_set\ncards:\n  condition-form:\n    status: complete\n    abilities: []\n"]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        value,
        &Value::Null,
        &Value::Null,
        "use-prerequisite",
    )
    .unwrap();
    game.load_internal_card(
        "condition-form",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

#[test]
fn bp16_059_evolution_requires_awakened_without_spending_pp_or_the_form_on_refusal() {
    for card in ["BP16-059", "BP16-SL17"] {
        for maximum in [6_u8, 7_u8] {
            let mut value = initial(card, "P1");
            value["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            value["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"condition-form"}]);
            let mut game = with_form(&value);
            let before = game.projection(View::Referee).unwrap();
            let result = game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            );
            if maximum == 7 {
                assert_eq!(
                    result.unwrap().outcome,
                    "resolved",
                    "awakened permits the printed evolution"
                );
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["pp"]["current"], 6_u8,
                    "only printed PP one is paid"
                );
                assert_eq!(
                    state["objects"]["source"]["power"], 5_u8,
                    "the matching form becomes effective"
                );
            } else {
                let refusal = result.unwrap();
                assert_eq!(
                    refusal.outcome, "cannot-evolve",
                    "six maximum PP is not awakened"
                );
                assert!(
                    refusal.events.is_empty(),
                    "the rejected evolution reveals nothing"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "the refusal spends no PP, form or evolution allowance"
                );
            }
        }
    }
}
