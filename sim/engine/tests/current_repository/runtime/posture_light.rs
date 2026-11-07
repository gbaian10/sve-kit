#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the complete printed Lala scenarios."
)]
use super::*;

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "posture-light",
    )
    .unwrap()
}

fn position(form: &str, super_evolution: bool) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP16-038","state":{"hp":1_u8,"max_hp":1_u8}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    if !super_evolution {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"id":"soil","card":"EBD02-T01","state":{"counters":{"stack_counter":1_u8}}}),
            );
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"blaze","card":"BP01-089"}]);
    }
    initial
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut play: CardPlay, accept: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Lala question {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    checks(game);
}

fn settle(game: &mut Game, accept: bool) {
    loop {
        checks(game);
        if let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("printed pending ability");
            };
            let play = game
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            finish(game, play, accept);
        } else if let Some(q) = game.internal_quick_choice().unwrap() {
            let play = game
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(game, play, accept);
        } else {
            break;
        }
    }
}

fn evolve(game: &mut Game, super_evolution: bool) {
    let pay = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
    )
    .unwrap();
}

#[test]
fn lala_all_prints_super_evolve_with_zero_or_two_targets_then_pay_stack_for_assail() {
    for form in ["BP16-039", "BP16-SL11"] {
        for enemies in [false, true] {
            let mut initial = position(form, true);
            if enemies {
                initial["players"]["P2"]["zones"]["field"] = json!([
                    {"id":"enemy-1","card":"BP01-171"},{"id":"enemy-2","card":"BP01-171"}
                ]);
            }
            let mut game = board(&initial);
            evolve(&mut game, true);
            settle(&mut game, true);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P2"]["field"],
                json!([]),
                "both formal victims are destroyed, while zero is legal (Q2097)"
            );
            let field = view["P1"]["field"].as_array().unwrap();
            assert_eq!(
                field.len(),
                2,
                "Stack +2 creates the real shard when no Stack exists (Q2772)"
            );
            let soil = field
                .iter()
                .find(|id| id.as_str() != Some("source"))
                .unwrap()
                .as_str()
                .unwrap();
            assert_eq!(view["objects"][soil]["name"], "大地の魔片");
            assert_eq!(
                view["objects"][soil]["counters"]["スタックカウンター"],
                2_u8
            );
            let ability = selection_with_rules().current[form]
                .program
                .card()
                .abilities[2]
                .node
                .id;
            let play = game
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, true);
            let after_activation = game.projection(View::Referee).unwrap();
            assert_eq!(
                after_activation["objects"][soil]["counters"]["スタックカウンター"], 1_u8,
                "activation pays one Stack"
            );
            assert!(
                after_activation["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("指定攻撃")),
                "the actual evolved form gains Assail"
            );
        }
    }
}

#[test]
fn lala_all_prints_lastword_may_pay_earth_rite_to_return_the_base_acted() {
    for form in ["BP16-039", "BP16-SL11"] {
        for accept in [false, true] {
            let mut game = board(&position(form, false));
            evolve(&mut game, false);
            settle(&mut game, true);
            let play = game
                .play_internal_card("blaze", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, accept);
            settle(&mut game, accept);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["field"],
                if accept {
                    json!(["source"])
                } else {
                    json!(["soil"])
                },
                "only accepted Earth Rite returns the source and consumes the last Stack"
            );
            assert_eq!(
                view["objects"]["source"]["generation"],
                if accept { 2_u8 } else { 1_u8 }
            );
            if accept {
                assert_eq!(
                    view["objects"]["source"]["acted"], true,
                    "entry is already acted"
                );
                assert_eq!(
                    view["objects"]["source"]["power"], 4_u8,
                    "the departed evolution returns as its true base"
                );
            }
            assert_eq!(
                view["P1"]["evolve_deck"],
                json!(["form"]),
                "the linked form returns to its owner's evolve deck"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 4_u8,
                "only ordinary evolution and the real killing spell spend PP"
            );
        }
    }
}
