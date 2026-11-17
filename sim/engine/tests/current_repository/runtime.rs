use super::*;
use alloc::collections::BTreeSet;
use core::fmt::Display;
use serde_json::Value;
use sve_engine::catalog::SelectedEffects;
use sve_engine::game::internal::{
    Activation, Answer, CardPlay, DecisionView, Fact, InputDomain, ResumeStage, SearchAnswer,
    SearchDomain,
};
use sve_engine::game::player::Viewer;
use sve_engine::game::player::events::{PlayerEvent, PlayerEventBatch};
use sve_engine::game::{Game, View};
use sve_engine::ir::*;

include!(concat!(env!("OUT_DIR"), "/runtime_modules.rs"));

const NUMBER: &str = "BP01-P25";

pub(super) fn setup(seat: &str, owner: &str, life: [u8; 2]) -> Value {
    let mut initial = json!({
        "history":"explicit",
        "turn":{"active":seat,"first_player":"P1","phase":"main",
            "elapsed_turns":{"P1":3_u8,"P2":3_u8}},
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":life[0]},
                "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
            "P2":{"leader":{"class":"ニュートラル","life":life[1]},
                "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
        }
    });
    initial["players"][seat]["zones"]["field"] =
        json!([{"id":"departing","card":NUMBER,"owner":owner}]);
    initial
}

pub(super) fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "current-runtime",
    )
    .unwrap()
}

pub(super) fn game_from_selection(
    initial: &Value,
    snapshot: &str,
    keywords: &str,
    input: &Value,
) -> Game {
    let selected = repository::select(snapshot, keywords, input).unwrap();
    Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "current-runtime",
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "A non-resolved play means the scenario itself is wrong."
)]
fn resolved(play: CardPlay) -> Activation {
    if let CardPlay::Resolved(result) = play {
        result
    } else {
        panic!("expected complete body")
    }
}

#[expect(
    clippy::panic,
    reason = "A non-pending domain means the scenario itself is wrong."
)]
fn pending(game: &mut Game, seat: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    assert_eq!(options.len(), 1, "exactly the permanent card's Lastword");
    assert_eq!(options[0].card, NUMBER, "pending option names the card");
    assert!(
        options[0].event.contains("LeaveField"),
        "pending option is the departure"
    );
    assert_eq!(options[0].line, 1, "pending option keeps the printed line");
    assert_eq!(
        question.actor.seat,
        seat_number(seat),
        "the controller chooses"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

// The fixture supplies only the external instruction producer. The real card's
// accepted selection, complete program and permanent source are never modified.
pub(super) fn producer(action: Action) -> WellFormedCardProgram {
    producer_steps(vec![action])
}

// Instruction N of the producer owns receipt N, so later steps can follow earlier ones.
pub(super) fn producer_steps(actions: Vec<Action>) -> WellFormedCardProgram {
    let source = SourceSpan {
        card_id: "producer".into(),
        card_no: "producer".into(),
        owner: "card".into(),
        scope: "producer".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: None,
        author_path: "/producer".into(),
    };
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
    };
    WellFormedCardProgram::check(CardProgram {
        card_play_costs: vec![],
        abilities: vec![Located {
            source: source.clone(),
            node: Ability {
                id: AbilityId(1),
                scope: ScopeId(1),
                kind: AbilityKind::Activated,
                qualification: AbilityQualification::Ordinary,
                source: origin.clone(),
                active_zones: vec![ZoneKind::Field],
                event_captures: vec![],
                play: Some(PlaySpec {
                    limit: None,
                    captures: vec![],
                    parameters: vec![],
                    costs: CostSpec::Ordered { steps: vec![] },
                }),
                trigger: None,
                declarations: vec![],
                body: Some(Program {
                    scope: ScopeId(1),
                    steps: actions
                        .into_iter()
                        .zip(1_u64..)
                        .map(|(action, number)| Located {
                            source: source.clone(),
                            node: Node::Instruction {
                                instruction: Box::new(Instruction {
                                    id: InstructionId(number),
                                    precondition: None,
                                    receipt: ReceiptId(number),
                                    actor: PlayerRef::Controller {},
                                    source: origin.clone(),
                                    context: InstructionContext::Effect,
                                    action,
                                }),
                            },
                        })
                        .collect(),
                }),
            },
        }],
    })
    .unwrap()
}

fn subjects() -> Selector {
    subjects_in(ZoneKind::Field)
}

fn subjects_in(zone: ZoneKind) -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::All,
            text_quantifier: TextQuantifier::All,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(2),
                maximum: Some(Nat(2)),
            },
        },
        zone,
        view: SelectionView::Current {},
        binding: Binding::new(50, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn movement(zone: OwnerZone) -> Action {
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
    }
}

fn producer_inputs() -> (String, String, Value) {
    let (mut snapshot, keywords, mut input) = inputs().clone();
    let face = serde_json::from_str::<Value>(
        snapshot
            .lines()
            .find(|line| serde_json::from_str::<Value>(line).unwrap()["number"] == NUMBER)
            .unwrap(),
    )
    .unwrap()["faces"][0]
        .clone();
    snapshot.push_str(&json!({"number":"producer","faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([
        "producer.yaml", "format: 1\nkind: effect_set\ncards:\n  producer:\n    status: complete\n    abilities: []\n"
    ]));
    (snapshot, keywords, input)
}

pub(super) fn selection_without_abilities(
    (snapshot, keywords, mut input): (String, String, Value),
    missing: &[&str],
) -> SelectedEffects {
    // Explicit omission keeps source-evidence fixtures independent of card migration progress.
    input["authors"].as_array_mut().unwrap().retain(|author| {
        !author["printings"].as_array().is_some_and(|printings| {
            printings.iter().any(|printing| {
                printing
                    .as_str()
                    .is_some_and(|number| missing.contains(&number))
            })
        }) && !author["card_no"]
            .as_str()
            .is_some_and(|number| missing.contains(&number))
    });
    repository::select(&snapshot, &keywords, &input).unwrap()
}

fn producer_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = producer_inputs();
        repository::select(&snapshot, &keywords, &input).unwrap()
    })
}

fn with_producer(initial: &Value, action: Action) -> Game {
    with_selected_producer(initial, action, producer_selection())
}

fn with_selected_producer(initial: &Value, action: Action, selected: &SelectedEffects) -> Game {
    with_selected_producer_program(initial, &producer(action), selected)
}

fn with_selected_producer_program(
    initial: &Value,
    program: &WellFormedCardProgram,
    selected: &SelectedEffects,
) -> Game {
    let mut initial = initial.clone();
    let seat = initial["turn"]["active"].as_str().unwrap().to_owned();
    initial["players"][&seat]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::from_selected_effects(
        selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "producer",
    )
    .unwrap();
    game.load_internal_card("producer", program).unwrap();
    game
}

fn activate(game: &mut Game, seat: &str) -> Activation {
    resolved(
        game.activate_internal_card(
            "producer",
            AbilityId(1),
            PlayerId {
                seat: if seat == "P1" { 1 } else { 2 },
            },
        )
        .unwrap(),
    )
}

#[test]
fn accepted_current_card_loads_and_projects_without_a_prototype_program() {
    for seat in ["P1", "P2"] {
        let mut game = game(&setup(seat, seat, [20, 20]));
        for view in [View::P1, View::P2, View::Referee] {
            assert_eq!(
                game.projection(view).unwrap()["objects"]["departing"]["card"],
                NUMBER
            );
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert!(
            failure(serde_json::to_value(&game)).contains("no save format"),
            "loaded internal cards keep rejecting saves"
        );
    }
}

fn seat_number(seat: &str) -> u32 {
    if seat == "P1" { 1 } else { 2 }
}

/// Checks the two simultaneous Destroy movements and returns the permanent card's
/// pre-move reference; only that card is owned by the other seat.
fn destroyed_holder(destruction: &Activation, seat: &str, owner: &str) -> ObjectRef {
    let movements = destruction
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Movement(movement) = fact {
                Some(movement)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(movements.len(), 2, "permanent card and producer both move");
    assert!(
        movements.iter().all(|movement| {
            movement.method == MoveMethod::Destroy
                && movement.movement.before.generation == Generation(0)
                && movement
                    .movement
                    .after
                    .is_some_and(|after| after.generation == Generation(1))
                && movement.cause
                    == (Cause::Effect {
                        execution: destruction.execution,
                    })
        }),
        "every movement is an effect Destroy that advances the generation"
    );
    let owner_id = seat_number(owner);
    let [current] = movements
        .iter()
        .filter(|movement| {
            movement
                .movement
                .actual_to
                .is_some_and(|to| to.player.seat == owner_id && to.kind == ZoneKind::Cemetery)
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    assert_eq!(
        current.controller_before.seat,
        seat_number(seat),
        "the activating seat controlled the card before it left"
    );
    current.movement.before
}

fn check_owner_cemetery(game: &Game, seat: &str, owner: &str) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["departing"]["card"], NUMBER,
        "the moved object stays the permanent card"
    );
    assert!(
        state[owner]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("departing")),
        "the card goes to its owner's cemetery"
    );
    assert_eq!(
        state[seat]["field"],
        json!([]),
        "the controller no longer has it on the field"
    );
}

/// The Lastword's source is the departed card's pre-move reference (generation 0),
/// the last-known information fixed when it left the field.
fn check_lastword_damage(result: &Activation, holder: ObjectRef, seat: &str) {
    assert_eq!(
        result.source.controller.seat,
        seat_number(seat),
        "the Lastword is controlled by the last controller"
    );
    assert_eq!(
        result.source.holder, holder,
        "the source is the departed card, not another object"
    );
    assert_eq!(
        result.source.holder.generation,
        Generation(0),
        "last-known information keeps the pre-move generation"
    );
    let damage = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(damage.len(), 2, "both leaders are hit");
    assert!(
        damage
            .iter()
            .all(|hit| hit.amount.0.get() == 2 && hit.source.object == result.source.holder),
        "each hit deals 2 from the departed card"
    );
    assert_eq!(
        damage[0].source, damage[1].source,
        "both hits come from the same source"
    );
    assert_ne!(
        damage[0].recipient, damage[1].recipient,
        "the two hits target different leaders"
    );
}

fn check_leader_life(game: &Game, life: [u8; 2]) {
    for perspective in [View::P1, View::P2, View::Referee] {
        let state = game.projection(perspective).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            life[0].saturating_sub(2),
            "P1 took 2 in every view"
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            life[1].saturating_sub(2),
            "P2 took 2 in every view"
        );
    }
}

/// Defeat is judged only after the whole simultaneous damage resolved (CR 1.2.1.1, 1.2.2).
fn check_ending(game: &Game, life: [u8; 2]) {
    let ending = &game.projection(View::Referee).unwrap()["game"];
    match life {
        [2, 2] => {
            assert_eq!(ending["ended"], true, "both defeated ends the game");
            assert_eq!(
                ending["winner"],
                Value::Null,
                "a simultaneous defeat is a draw"
            );
            assert_eq!(ending["by"], "1.2.2", "the draw cites CR 1.2.2");
        }
        [2, 3] => {
            assert_eq!(ending["ended"], true, "P1 is defeated");
            assert_eq!(ending["winner"], "P2", "the survivor wins");
        }
        [3, 2] => {
            assert_eq!(ending["ended"], true, "P2 is defeated");
            assert_eq!(ending["winner"], "P1", "the survivor wins");
        }
        _ => assert_ne!(ending["ended"], true, "no leader is at zero life"),
    }
}

#[test]
fn ordinary_destroy_uses_owner_cemetery_lastword_lki_and_simultaneous_leader_damage() {
    // CR 4.1.6, 5.6.1, 12.5.1/.3, 10.11.1, 10.12.1.1/.2.2.
    for seat in ["P1", "P2"] {
        let owner = if seat == "P1" { "P2" } else { "P1" };
        for life in [[3_u8, 3_u8], [2, 3], [3, 2], [2, 2]] {
            let mut game = with_producer(
                &setup(seat, owner, life),
                Action::Destroy {
                    objects: subjects(),
                },
            );
            let destruction = activate(&mut game, seat);
            let holder = destroyed_holder(&destruction, seat, owner);

            assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
            game.resume_internal_card().unwrap();
            check_owner_cemetery(&game, seat, owner);
            let result = pending(&mut game, seat);
            check_lastword_damage(&result, holder, seat);
            let before_check = game.projection(View::Referee).unwrap();
            assert_ne!(before_check["game"]["ended"], true);
            check_leader_life(&game, life);

            let check = game.resume_internal_card().unwrap();
            check_ending(&game, life);
            let defeats = usize::from(life[0] == 2) + usize::from(life[1] == 2);
            assert_eq!(
                check
                    .events
                    .iter()
                    .filter(|event| event["kind"] == "敗北")
                    .count(),
                defeats
            );
            check_leader_life(&game, life);
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn games_sharing_a_selection_keep_added_programs_and_positions_independent() {
    let initial = setup("P1", "P1", [20, 20]);
    let mut banish = with_producer(&initial, movement(OwnerZone::Banish));
    let mut bounce = with_producer(&initial, movement(OwnerZone::Hand));
    let untouched = bounce.projection(View::Referee).unwrap();

    activate(&mut banish, "P1");
    banish.resume_internal_card().unwrap();
    assert!(
        bounce.projection(View::Referee).unwrap() == untouched,
        "the other game keeps its original position"
    );
    assert!(
        banish.projection(View::Referee).unwrap()["P1"]["banish"]
            .as_array()
            .unwrap()
            .contains(&json!("departing"))
    );

    activate(&mut bounce, "P1");
    bounce.resume_internal_card().unwrap();
    assert!(
        bounce.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("departing"))
    );
    assert!(
        banish.projection(View::Referee).unwrap()["P1"]["banish"]
            .as_array()
            .unwrap()
            .contains(&json!("departing"))
    );
}

#[test]
fn banish_bounce_and_non_field_cemetery_moves_do_not_trigger_lastword() {
    // CR 5.7.1 defines banishment as movement to the owner's banish zone.
    // CR 12.5.1/.3 requires a field -> cemetery departure, independent of method.
    for seat in ["P1", "P2"] {
        for zone in [OwnerZone::Banish, OwnerZone::Hand] {
            let mut game = with_producer(&setup(seat, seat, [20, 20]), movement(zone));
            activate(&mut game, seat);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seat]["leader"]["life"], 20_u8);
            let destination = if zone == OwnerZone::Banish {
                "banish"
            } else {
                "hand"
            };
            assert!(
                state[seat][destination]
                    .as_array()
                    .unwrap()
                    .contains(&json!("departing"))
            );
        }
        let mut initial = setup(seat, seat, [20, 20]);
        initial["players"][seat]["zones"]["ex"] = initial["players"][seat]["zones"]["field"].take();
        initial["players"][seat]["zones"]["field"] = json!([]);
        let mut action = movement(OwnerZone::Cemetery);
        if let Action::Move { objects, .. } = &mut action {
            *objects = subjects_in(ZoneKind::Ex);
        }
        let mut game = with_producer(&initial, action);
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn battle_lethal_damage_produces_each_current_lastword_once() {
    // CR 8.4.9/.9.1, 11.3.1: simultaneous combat damage and rule destruction.
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = setup(seat, seat, [20, 20]);
        initial["players"][opponent]["zones"]["field"] = json!([
            {"id":"defender","card":NUMBER,"state":{"acted":true}}
        ]);
        let mut game = game(&initial);
        let attack = game
            .internal_rule_action(
                &json!({"do":"attack","attacker":"departing","target":"defender"}),
            )
            .unwrap();
        assert_eq!(attack.outcome, "resolved");
        let question = game.internal_quick_choice().unwrap().unwrap();
        assert!(matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ));
        pending(&mut game, seat);
        game.resume_internal_card().unwrap();
        pending(&mut game, opponent);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        for view in [View::P1, View::P2, View::Referee] {
            let state = game.projection(view).unwrap();
            assert_eq!(state["P1"]["leader"]["life"], 16_u8);
            assert_eq!(state["P2"]["leader"]["life"], 16_u8);
        }
    }
}

/// Diagnostic of a rejection; an unexpected success yields an empty string, never a card dump.
fn failure<T, E>(result: Result<T, E>) -> String
where
    E: Display,
{
    result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default()
}

fn construct(selected: SelectedEffects, seed: &str) -> String {
    failure(Game::from_selected_effects(
        selected,
        &setup("P1", "P1", [20, 20]),
        &Value::Null,
        &Value::Null,
        seed,
    ))
}

#[test]
fn current_admission_rejects_foreign_snapshot_and_program() {
    let (snapshot, keywords, input) = inputs();
    let mut cleared = repository::select(snapshot, keywords, input).unwrap();
    assert_eq!(construct(cleared.clone(), "warm-cleared"), "");
    cleared.current.clear();
    assert!(
        construct(cleared, "changed").contains("current selection changed after acceptance"),
        "removing an accepted candidate rejects the whole construction"
    );
    // The prototype closure is part of the accepted selection, even when its content is equal.
    let mut replaced = repository::select(snapshot, keywords, input).unwrap();
    assert_eq!(construct(replaced.clone(), "warm-replaced"), "");
    replaced.prototypes = repository::select(snapshot, keywords, input)
        .unwrap()
        .prototypes;
    assert!(
        construct(replaced, "replaced").contains("current selection changed after acceptance"),
        "replacing the prototype catalogue rejects the whole construction"
    );
    // A current candidate cannot gain admission through an unrelated prototype catalogue.
    let accepted = repository::select(snapshot, keywords, input).unwrap();
    let mut plain = Game::new(
        accepted.prototypes,
        &setup("P1", "P1", [20, 20]),
        &Value::Null,
        &Value::Null,
        "foreign",
    )
    .unwrap();
    assert!(
        failure(plain.load_internal_card(NUMBER, &accepted.current[NUMBER].program))
            .contains("no authored program"),
        "a plain catalogue has no program to admit the current card against"
    );
    let strict_number = "BP10-122";
    assert_ne!(accepted.current[strict_number].source_version, "direct");
    let changed = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card["number"] == strict_number {
                card["faces"][0]["name"] = json!("foreign");
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let foreign = repository::select(&changed, keywords, input).unwrap();
    assert!(
        construct(foreign, "foreign").contains("no unique snapshot source"),
        "source-rebuilt candidates still reject different snapshot face text"
    );
    let changed_faces = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card["number"] == NUMBER {
                let face = card["faces"][0].clone();
                card["faces"].as_array_mut().unwrap().push(face);
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let direct_foreign = repository::select(&changed_faces, keywords, input).unwrap();
    assert!(
        construct(direct_foreign, "foreign-direct")
            .contains("current snapshot closure or prototype overlap"),
        "direct candidates still reject a snapshot with an extra face"
    );
}

#[test]
fn accepted_current_follower_can_be_played_from_hand() {
    for seat in ["P1", "P2"] {
        let mut initial = setup(seat, seat, [20, 20]);
        initial["players"][seat]["zones"]["hand"] =
            initial["players"][seat]["zones"]["field"].take();
        initial["players"][seat]["zones"]["field"] = json!([]);
        let mut game = game(&initial);
        let entry = game
            .play_internal_card(
                "departing",
                PlayerId {
                    seat: if seat == "P1" { 1 } else { 2 },
                },
            )
            .unwrap();
        assert!(matches!(entry, CardPlay::Entered(_)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["field"], json!(["departing"]));
        assert_eq!(state[seat]["pp"]["current"], 7_u8);
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

fn draw_position() -> Value {
    json!({
        "history":"explicit",
        "turn":{"active":"P1","first_player":"P1","phase":"main",
            "elapsed_turns":{"P1":3_u8,"P2":3_u8}},
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":20_u8},
                "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
            "P2":{"leader":{"class":"ニュートラル","life":20_u8},
                "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
        }
    })
}

fn projected_zone_count(zone: &Value) -> u64 {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|card| card.get("filler").and_then(Value::as_u64).unwrap_or(1))
        .sum()
}

// Helpers shared verbatim by several runtime scenario files.
fn ability(card: &str, index: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities[index]
        .node
        .id
}

fn accept(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            break;
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn activate_printed(game: &mut Game, card: &str) -> CardPlay {
    game.activate_internal_card("source", printed_id(card), actor())
        .unwrap()
}

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn activation(number: &str) -> AbilityId {
    selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn advance_entry() -> Action {
    Action::Move {
        objects: Selector::Zone {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                text_quantifier: TextQuantifier::All,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
            },
            zone: ZoneKind::Evolve,
            view: SelectionView::Current {},
            binding: Binding::new(50, ScopeId(1)),
            predicate: Box::new(Expr::Literal(Bool(true))),
        },
        movement: MoveSpec {
            to: MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                },
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

fn aliases(representative: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[representative].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(number, _)| number.clone())
        .collect()
}

fn all_other_followers() -> RecipientSet {
    let mut objects = subjects();
    if let Selector::Zone {
        binding, predicate, ..
    } = &mut objects
    {
        **predicate = Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }));
    }
    RecipientSet::Objects {
        objects: Selector::Difference {
            input: Box::new(objects),
            except: Box::new(Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }),
        },
    }
}

fn append(snapshot: &mut String, input: &mut Value, number: &str, face: &Value) {
    snapshot.push_str(&json!({"number":number,"faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}

fn cards(prefix: &str, card: &str, count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| json!({"id":format!("{prefix}{i}"),"card":card}))
        .collect()
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn committed(game: &Game, viewer: Viewer) -> Vec<PlayerEventBatch> {
    game.event_batches(viewer, None).unwrap().cloned().collect()
}

fn count(view: &Value, zone: &str, name: &str) -> usize {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

fn create_token(name: &str) -> Action {
    Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplate::Fixed {
                template: TokenTemplateId(name.into()),
            },
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    }
}

fn damage() -> Action {
    Action::Damage {
        recipients: all_other_followers(),
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(4)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn damage_own() -> Action {
    Action::Damage {
        recipients: RecipientSet::Objects {
            objects: own_followers(),
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(2)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn declare(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn definition(card: &str, kind: AbilityKind, nth: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|ability| ability.node.kind == kind)
        .nth(nth)
        .unwrap()
        .node
        .id
}

fn discard_all() -> WellFormedCardProgram {
    producer(Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::All {
            eligible: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::All,
                },
                zone: ZoneKind::Hand,
                view: SelectionView::Current {},
                binding: Binding::new(400, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        },
    })
}

fn face(name: &str, kind: &str, cost: u64, class: &str, traits: &[&str]) -> Value {
    let follower = kind == "フォロワー";
    json!({"name":name,"card_type":kind,"card_class":class,"cost":cost.to_string(),
        "power":if follower {"2"} else {"-"},"hp":if follower {"10"} else {"-"},
        "traits":traits,"text":Value::Null,"sections":[]})
}

fn field_damage(view: &Value) {
    assert_eq!(
        view["objects"]["source"]["acted"], true,
        "the source pays its act cost"
    );
    assert_eq!(
        view["objects"]["enemy"]["damage"], 1_u8,
        "each opposing follower receives one damage"
    );
    assert_eq!(
        view["P2"]["leader"]["life"], 20_u8,
        "the opposing leader is not a follower"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["soil"]),
        "two removed Stack immediately clear their holder"
    );
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn finish_mode(game: &mut Game, mut step: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            let mut members: Vec<_> = domain.members.iter().collect();
            members.sort_by_key(|member| {
                if member.card.as_deref() == Some("BP01-092") {
                    0_u8
                } else if member.card.as_deref() == Some("BP01-037") {
                    2
                } else {
                    3
                }
            });
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::Targets(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &q.domain {
            let members: Vec<_> = domain.members.iter().collect();
            assert!(
                members.iter().all(|member| member.player == actor()),
                "Only the controller can supply these cost materials"
            );
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::CostMaterials(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::Search(domain) = &q.domain {
            let limit = usize::try_from(
                domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0,
            )
            .unwrap();
            Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(limit).map(|c| c.handle).collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if matches!(&q.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![mode])
        } else if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::TokenCapacity { .. }) {
            Answer::TokenCapacity(0)
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

#[expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected decision invalidates the scenario."
)]
fn finish_with(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Search(domain) => Answer::Search(domain.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        domain
                            .cards
                            .iter()
                            .take(take(&domain.cardinality, domain.cards.len()))
                            .map(|card| card.handle)
                            .collect(),
                    )
                },
                |groups| {
                    // Each printed group greedily takes cards no earlier group used.
                    let mut used = Vec::new();
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|group| {
                                let picked: Vec<u64> = group
                                    .handles
                                    .iter()
                                    .copied()
                                    .filter(|handle| !used.contains(handle))
                                    .take(take(&group.cardinality, group.handles.len()))
                                    .collect();
                                used.extend(picked.iter().copied());
                                picked
                            })
                            .collect(),
                    )
                },
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|card| card.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Integer(bounds) => {
                Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum))
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn has_keyword(state: &Value, id: &str, keyword: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

fn history(game: &Game, viewer: Viewer) -> Vec<PlayerEvent> {
    game.events(viewer, None).unwrap().cloned().collect()
}

fn in_hand(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}

fn line(number: &str, printed: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == printed)
        .unwrap()
        .node
        .id
}

fn most(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |n| usize::try_from(n.0).unwrap())
        .min(available)
}

fn next_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if game.internal_quick_choice().unwrap().is_none() {
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap();
    }
    damage_prevent::pass_quick(game);
}

fn opposite(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn order(game: &mut Game, question: &DecisionView, remaining: usize) {
    let InputDomain::Order(cards) = &question.domain else {
        panic!("only the looked remainder is ordered")
    };
    assert_eq!(
        cards.len(),
        remaining,
        "the moved card and unobserved tail are excluded from the private remainder"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
}

fn own_followers() -> Selector {
    let mut selector = subjects();
    if let Selector::Zone { players, .. } = &mut selector {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    Selector::Difference {
        input: Box::new(selector),
        except: Box::new(Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        }),
    }
}

fn paid(view: &Value, before: &Value, n: usize, pp: i64) {
    assert_eq!(
        view["P1"]["ex"].as_array().unwrap().len(),
        5_usize.checked_sub(n).unwrap(),
        "the exact Lesson amount is consumed"
    );
    assert_eq!(
        view["P1"]["pp"]["current"].as_i64().unwrap(),
        before["P1"]["pp"]["current"]
            .as_i64()
            .unwrap()
            .checked_sub(pp)
            .unwrap(),
        "only the additional printed PP is paid for Lesson"
    );
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn pass_quick(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("Quick did not finish");
}

fn pp(game: &Game) -> u64 {
    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap()
}

/// The `index`-th printed activated ability of the card (0-based).
fn printed_activation(card: &str, index: usize) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .nth(index)
        .unwrap()
        .node
        .id
}

fn printed_id(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id
}

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

fn push(initial: &mut Value, player: &str, zone: &str, object: Value) {
    let slot = &mut initial["players"][player]["zones"][zone];
    if slot.is_null() {
        *slot = json!([]);
    }
    slot.as_array_mut().unwrap().push(object);
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed question")
    };
    q
}

/// Rejected answers must leave the same question open.
fn rejects(game: &mut Game, question: &DecisionView, handles: Vec<u64>) {
    let before = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "a rejected answer leaves the choice open"
    );
}

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "light-grants",
    )
    .unwrap()
}

/// Takes the first legal cards, one per printed name, group by group.
fn search(domain: &SearchDomain) -> SearchAnswer {
    let mut names = BTreeSet::new();
    let mut pick = |handles: &[u64], cardinality: &Cardinality| -> Vec<u64> {
        let limit = take(cardinality, handles.len());
        handles
            .iter()
            .copied()
            .filter(|handle| {
                let name = &domain.cards[usize::try_from(*handle).unwrap()].name;
                names.insert(name.clone())
            })
            .take(limit)
            .collect()
    };
    match &domain.groups {
        None => SearchAnswer::Ungrouped(pick(
            &domain
                .cards
                .iter()
                .map(|card| card.handle)
                .collect::<Vec<_>>(),
            &domain.cardinality,
        )),
        Some(groups) => SearchAnswer::Grouped(
            groups
                .iter()
                .map(|group| pick(&group.handles, &group.cardinality))
                .collect(),
        ),
    }
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

fn source(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn strike_with_producer(game: &mut Game) -> Activation {
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    )
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn titled(mut initial: Value) -> Value {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial
}

#[expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed choice")
    };
    question
}
