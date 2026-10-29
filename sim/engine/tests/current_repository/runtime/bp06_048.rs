#![expect(
    clippy::panic,
    reason = "A malformed typed search response means the scenario cannot provide its assertion target."
)]

use super::*;
use sve_engine::game::internal::{InstructionOutcome, SearchAnswer};
use sve_engine::ir::SearchOutcome;

const CARD: &str = "BP06-048";
const SPELL: &str = "BP06-042";

fn search_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| selection_without_abilities(producer_inputs(), &[SPELL]))
}

fn position(controller: &str, owner: &str, deck: Value) -> Value {
    let mut initial = json!({
        "history":"explicit",
        "turn":{"active":controller,"first_player":"P1","phase":"main",
            "elapsed_turns":{"P1":3_u8,"P2":3_u8}},
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":20_u8},
                "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
            "P2":{"leader":{"class":"ニュートラル","life":20_u8},
                "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
        }
    });
    initial["players"][controller]["zones"]["field"] =
        json!([{"id":"departing","card":CARD,"owner":owner}]);
    initial["players"][controller]["zones"]["deck"] = deck;
    initial
}

fn game_with_external_movement(initial: &Value, action: Action) -> Game {
    let mut initial = initial.clone();
    let controller = initial["turn"]["active"].as_str().unwrap().to_owned();
    initial["players"][&controller]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let selected = search_selection().clone();
    let mut game = Game::from_selected_effects(
        selected,
        &initial,
        &Value::Null,
        &Value::Null,
        "bp06-048-search",
    )
    .unwrap();
    game.load_internal_card("producer", &producer(action))
        .unwrap();
    game
}

fn game_with_destroyer(initial: &Value) -> Game {
    game_with_external_movement(
        initial,
        Action::Destroy {
            objects: subjects(),
        },
    )
}

fn destroy_and_wait_for_search(game: &mut Game, controller: &str, owner: &str) -> Vec<ObjectRef> {
    let destruction = activate(game, controller);
    let destroyed = destruction
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            let Fact::Movement(movement) = fact else {
                return None;
            };
            (movement.method == MoveMethod::Destroy
                && movement.movement.before.generation == Generation(0)
                && movement.movement.actual_to.is_some_and(|to| {
                    to.player.seat == seat_number(owner) && to.kind == ZoneKind::Cemetery
                }))
            .then_some(movement.movement.before)
        })
        .collect::<Vec<_>>();
    assert!(
        !destroyed.is_empty(),
        "a destroyed card reaches its owner's cemetery"
    );
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("lastword pending domain")
    };
    let [option] = options.as_slice() else {
        panic!("one BP06-048 lastword")
    };
    assert_eq!(option.card, CARD, "the pending trigger is BP06-048");
    assert_eq!(option.line, 1, "the trigger preserves its printed line");
    assert!(
        option.event.contains("LeaveField"),
        "the trigger is caused by a field departure"
    );
    assert_eq!(
        question.actor.seat,
        seat_number(controller),
        "the pre-departure controller selects the Lastword"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    assert!(
        matches!(play, CardPlay::Awaiting(_)),
        "search awaits choice"
    );
    destroyed
}

fn resolve_search(game: &mut Game, controller: &str, handles: Vec<u64>) -> Activation {
    let decision = game
        .internal_decision(if controller == "P1" {
            View::P1
        } else {
            View::P2
        })
        .unwrap();
    assert_eq!(
        decision.actor.seat,
        seat_number(controller),
        "the pre-departure controller answers the Search"
    );
    assert!(
        matches!(decision.domain, InputDomain::Search(_)),
        "Lastword resolution waits on a Search domain"
    );
    resolved(
        game.answer_internal_card(
            decision.actor,
            decision.token,
            Answer::Search(SearchAnswer::Ungrouped(handles)),
        )
        .unwrap(),
    )
}

fn search_outcome(result: &Activation) -> &SearchOutcome {
    let Some(InstructionOutcome::Search(outcome)) = result
        .progress
        .receipts
        .values()
        .find(|outcome| matches!(outcome, InstructionOutcome::Search(_)))
    else {
        panic!("search receipt")
    };
    outcome
}

fn shuffled_for(result: &Activation, seat: &str) -> usize {
    result
        .progress
        .facts
        .iter()
        .filter(
            |fact| matches!(fact, Fact::Shuffled { player } if player.seat == seat_number(seat)),
        )
        .count()
}

#[test]
fn bp06_048_reports_the_missing_ability_source_evidence_for_a_real_search_match() {
    // The real search candidate is deliberately unselected even after its author migration.
    for (controller, owner) in [("P1", "P2"), ("P2", "P1")] {
        let initial = position(controller, owner, json!([{"id":"spell","card":SPELL}]));
        let mut game = game_with_destroyer(&initial);
        let before = [View::P1, View::P2, View::Referee]
            .map(|view| game.projection(view).map_err(|error| error.to_string()));
        let error = failure(game.activate_internal_card(
            "producer",
            AbilityId(1),
            PlayerId {
                seat: seat_number(controller),
            },
        ));
        eprintln!("BP06-048 {controller} blocked before Search: {error}");
        assert!(error.contains("unloaded whole card BP06-042/0"));
        assert!(error.contains("holder spell generation 0"));
        assert!(error.contains("active zones"));
        assert!(error.contains("current ability definitions unavailable"));
        assert_eq!(
            [View::P1, View::P2, View::Referee]
                .map(|view| game.projection(view).map_err(|failure| failure.to_string())),
            before
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert!(
            game.internal_decision(if controller == "P1" {
                View::P1
            } else {
                View::P2
            })
            .is_none()
        );
        assert_eq!(game.internal_card_stage(), None);
    }
}

#[test]
fn bp06_048_searches_and_shuffles_when_no_match_or_the_deck_is_empty() {
    // CR 5.8.1-.2, 5.9.1.1; official BP06-048 FAQ Q816 requires searching and shuffling even when absent.
    for deck in [
        json!([]),
        json!([{"id":"one","card":"BP06-048"}]),
        json!([
            {"id":"unrelated-1","card":"BP06-048"},
            {"id":"unrelated-2","card":"BP06-048"}
        ]),
    ] {
        let deck_size = deck.as_array().unwrap().len();
        // The Lastword master is the pre-departure controller, even when the other seat owns the card.
        for (controller, owner) in [("P1", "P1"), ("P2", "P2"), ("P1", "P2"), ("P2", "P1")] {
            let other = if controller == "P1" { "P2" } else { "P1" };
            let initial = position(controller, owner, deck.clone());
            let mut game = game_with_destroyer(&initial);
            let departed = destroy_and_wait_for_search(&mut game, controller, owner);
            let view = if controller == "P1" {
                View::P1
            } else {
                View::P2
            };
            let decision = game.internal_decision(view).unwrap();
            let InputDomain::Search(domain) = &decision.domain else {
                panic!("search domain")
            };
            assert_eq!(domain.cards.len(), 0, "no card has the fixed name");
            let result = resolve_search(&mut game, controller, vec![]);
            assert_eq!(
                result.source.controller.seat,
                seat_number(controller),
                "Lastword source keeps its pre-departure controller"
            );
            assert_eq!(
                result.source.holder.generation,
                Generation(0),
                "Lastword source keeps the pre-departure object generation"
            );
            // The producer also leaves for its controller's cemetery, so only a foreign-owned card is unique;
            // the pending option already fixed that this holder's card is BP06-048.
            assert_eq!(departed.len(), if controller == owner { 2 } else { 1 });
            assert!(
                departed.contains(&result.source.holder),
                "Lastword source is one of the objects destroyed into its owner's cemetery"
            );
            assert_eq!(
                search_outcome(&result).found.len(),
                0,
                "the search found nothing"
            );
            assert_eq!(
                search_outcome(&result).moved.len(),
                0,
                "the search moved nothing"
            );
            assert_eq!(shuffled_for(&result, controller), 1);
            assert_eq!(
                shuffled_for(&result, other),
                0,
                "only the searched deck is shuffled"
            );
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            for seat in [controller, other] {
                assert_eq!(
                    state[seat]["hand"].as_array().unwrap().len(),
                    0,
                    "a miss leaves every hand unchanged"
                );
            }
            assert_eq!(
                state[controller]["deck"].as_array().unwrap().len(),
                deck_size,
                "a miss keeps every searched card in the deck"
            );
            assert_eq!(state[other]["deck"].as_array().unwrap().len(), 0);
        }
    }
}

#[test]
fn bp06_048_does_not_trigger_when_it_leaves_field_for_banish_or_hand() {
    // CR 12.5.1: Lastword requires this card to move from the field to its owner's cemetery.
    for controller in ["P1", "P2"] {
        for (zone, name) in [(OwnerZone::Banish, "banish"), (OwnerZone::Hand, "hand")] {
            let initial = position(controller, controller, json!([]));
            let mut game = game_with_external_movement(
                &initial,
                Action::Move {
                    objects: subjects(),
                    movement: MoveSpec {
                        to: MoveDestination::Owners { zone },
                        initial_posture: None,
                        entry_effects: vec![],
                        position: None,
                        order: None,
                    },
                    reason: MoveReason::Put,
                },
            );
            let movement = activate(&mut game, controller);
            assert_eq!(
                movement
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
                    .count(),
                0,
                "a non-cemetery departure does not begin a search"
            );
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state[controller][name]
                    .as_array()
                    .unwrap()
                    .contains(&json!("departing"))
            );
            assert_eq!(state[controller]["cemetery"], json!([]));
            assert_eq!(
                state[controller]["deck"].as_array().unwrap().len(),
                0,
                "the controller's empty deck remains empty"
            );
        }
    }
}
