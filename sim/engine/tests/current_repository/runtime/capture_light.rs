use super::damage_prevent::{field_name, named_field};
use super::*;
use sve_engine::game::internal::DamageShare;

fn position(number: &str, actions: Vec<Action>) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"producer","card":"producer"},{"id":"other","card":number}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-171"},{"id":"next","card":"BP01-118"}]);
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "capture-light",
    )
    .unwrap();
    game.load_internal_card("producer", &producer_steps(actions))
        .unwrap();
    game
}

fn kill(batch: bool) -> Action {
    Action::Destroy {
        objects: if batch {
            Selector::Union {
                sets: vec![
                    field_name("セージコマンダー"),
                    named_field("イベイジョンマーセナリー", 51),
                ],
            }
        } else {
            field_name("セージコマンダー")
        },
    }
}

#[test]
fn mercenary_printings_link_only_their_own_damage_and_preserve_sealed_departures() {
    for number in ["BP20-025", "BP20-P15"] {
        for batch in [false, true] {
            let mut game = position(number, vec![kill(batch)]);
            let entry = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            event_subject_capture::finish(&mut game, entry);
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("Fanfare")
            };
            let CardPlay::Awaiting(targets) = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap()
            else {
                panic!("formal target")
            };
            let CardPlay::Awaiting(shares) = game
                .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
                .unwrap()
            else {
                panic!("six damage")
            };
            let damage = game
                .answer_internal_card(
                    shares.actor,
                    shares.token,
                    Answer::Distribution(vec![DamageShare {
                        handle: 0,
                        amount: Nat(6),
                    }]),
                )
                .unwrap();
            event_subject_capture::finish(&mut game, damage);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["victim"]["damage"],
                6_u8
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
            activate(&mut game, "P1");
            event_cemetery::finish_checks(&mut game);
            let departure = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(departures) = &departure.domain else {
                panic!("source-linked departure")
            };
            assert_eq!(departures.len(), 1, "the other copy never dealt damage");
            let departure_resolution = game
                .answer_internal_card(
                    departure.actor,
                    departure.token,
                    Answer::Pending(departures[0].handle),
                )
                .unwrap();
            event_subject_capture::finish(&mut game, departure_resolution);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"], json!(["top"]));
            assert_eq!(state["P1"]["pp"]["current"], 5_u8);
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn friendly_damage_from_a_different_card_never_satisfies_a_mercenarys_self_source_condition() {
    let mut game = position(
        "BP20-025",
        vec![
            Action::Damage {
                recipients: RecipientSet::Objects {
                    objects: field_name("セージコマンダー"),
                },
                amount: DamageAmount::Uniform {
                    amount: Expr::Literal(Int(1)),
                },
                flags: DamageFlags::Ability,
                source: ObjectView::SourceInformation {},
                batch: BatchPolicy::Instruction {},
            },
            Action::Destroy {
                objects: named_field("セージコマンダー", 51),
            },
        ],
    );
    activate(&mut game, "P1");
    event_cemetery::finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!([])
    );
}
