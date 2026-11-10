#![expect(
    clippy::panic,
    reason = "The real Oceanus observer must ask for its printed modes."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn question(game: &mut Game) -> DecisionView {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution observer")
    };
    let option = options
        .iter()
        .find(|option| option.card == "BP09-112")
        .unwrap();
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
    else {
        panic!("Oceanus modes")
    };
    choice
}

// Reconciliation of real sources dominates the first run; both JP crests share one selection.
#[test]
fn sham_naqoa_crest_choice_any_all_printings_replace_the_real_oceanus_count() {
    for number in ["BP20-P65", "BP20-T06"] {
        for all in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"observer","card":"BP09-112"},{"id":"subject","card":"BP02-118"}
            ]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"crest","card":number}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP02-119"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "crest-choice-any",
            )
            .unwrap();
            let original = game.projection(View::Referee).unwrap();
            let enemy_hp = original["objects"]["enemy"]["hp"].as_i64().unwrap();
            game.internal_rule_action(
                &json!({"do":"evolve","source":"subject","evolve_card":"form","pay":{"pp":3_u8}}),
            )
            .unwrap();
            event_cemetery::finish_checks(&mut game);
            let choice = question(&mut game);
            let InputDomain::Modes {
                modes,
                cardinality,
                count,
            } = &choice.domain
            else {
                panic!("modes")
            };
            assert_eq!(modes.len(), 2_usize);
            assert_eq!(*count, Nat(2));
            assert_eq!(
                *cardinality,
                Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(2))
                }
            );
            let selected = if all {
                vec![ModeId(2), ModeId(1)]
            } else {
                vec![]
            };
            let declared = game
                .answer_internal_card(choice.actor, choice.token, Answer::Modes(selected))
                .unwrap();
            if all {
                let CardPlay::Awaiting(target) = declared else {
                    panic!("the selected damage mode declares its formal target")
                };
                let InputDomain::Targets(domain) = &target.domain else {
                    panic!("formal target")
                };
                let handle = domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("BP01-037"))
                    .unwrap()
                    .handle;
                resolved(
                    game.answer_internal_card(
                        target.actor,
                        target.token,
                        Answer::Targets(vec![handle]),
                    )
                    .unwrap(),
                );
            } else {
                resolved(declared);
            }
            event_cemetery::finish_checks(&mut game);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["pp"]["current"], 7_u8);
            assert_eq!(
                after["P1"]["leader"]["life"],
                if all { 16_u8 } else { 15_u8 }
            );
            assert_eq!(
                after["objects"]["enemy"]["hp"].as_i64().unwrap(),
                enemy_hp - if all { 2_i64 } else { 0_i64 }
            );
        }
    }
}
