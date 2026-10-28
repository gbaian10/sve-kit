use super::*;
use core::fmt::Display;
use serde_json::Value;
use sve_engine::catalog::SelectedEffects;
use sve_engine::game::internal::{Activation, Answer, CardPlay, Fact, InputDomain, ResumeStage};
use sve_engine::game::{Game, View};
use sve_engine::ir::*;

// Keep sorted; add new modules in order.

#[path = "runtime/bp01_037.rs"]
mod bp01_037;

#[path = "runtime/bp06_048.rs"]
mod bp06_048;

#[path = "runtime/bp10_122.rs"]
mod bp10_122;

#[path = "runtime/bp14_098.rs"]
mod bp14_098;

#[path = "runtime/bp19_p32.rs"]
mod bp19_p32;

#[path = "runtime/c2_01.rs"]
mod c2_01;

#[path = "runtime/c2_02.rs"]
mod c2_02;

#[path = "runtime/c2_03.rs"]
mod c2_03;

#[path = "runtime/c2_04.rs"]
mod c2_04;

#[path = "runtime/c2_05.rs"]
mod c2_05;

#[path = "runtime/conditions_quick.rs"]
mod conditions_quick;

#[path = "runtime/cp02_068.rs"]
mod cp02_068;

#[path = "runtime/deck_top.rs"]
mod deck_top;

#[path = "runtime/ecp01_002.rs"]
mod ecp01_002;

#[path = "runtime/ep_increase.rs"]
mod ep_increase;

#[path = "runtime/field_actions.rs"]
mod field_actions;

#[path = "runtime/hand_discard.rs"]
mod hand_discard;

#[path = "runtime/leader_damage.rs"]
mod leader_damage;

#[path = "runtime/leader_life.rs"]
mod leader_life;

#[path = "runtime/life_cost.rs"]
mod life_cost;

#[path = "runtime/live_conditions.rs"]
mod live_conditions;

#[path = "runtime/max_pp.rs"]
mod max_pp;

#[path = "runtime/pp_recover.rs"]
mod pp_recover;

#[path = "runtime/recipient_targets.rs"]
mod recipient_targets;

#[path = "runtime/rescan_001.rs"]
mod rescan_001;

#[path = "runtime/rescan_002.rs"]
mod rescan_002;

#[path = "runtime/resolution_choice.rs"]
mod resolution_choice;

#[path = "runtime/reveal_hand.rs"]
mod reveal_hand;

#[path = "runtime/token_trio.rs"]
mod token_trio;

#[path = "runtime/unloaded.rs"]
mod unloaded;

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
    let (snapshot, keywords, input) = inputs();
    game_from_selection(initial, &snapshot, &keywords, &input)
}

pub(super) fn game_from_selection(
    initial: &Value,
    snapshot: &str,
    keywords: &str,
    input: &Value,
) -> Game {
    let selected = Catalog::from_current_selection(snapshot, keywords, input).unwrap();
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

fn with_producer(initial: &Value, action: Action) -> Game {
    let (mut snapshot, keywords, mut input) = inputs();
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
    let mut initial = initial.clone();
    let seat = initial["turn"]["active"].as_str().unwrap().to_owned();
    initial["players"][&seat]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut game =
        Game::from_selected_effects(selected, &initial, &Value::Null, &Value::Null, "producer")
            .unwrap();
    game.load_internal_card("producer", &producer(action))
        .unwrap();
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
    let mut cleared = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    cleared.current.clear();
    assert!(
        construct(cleared, "changed").contains("current selection changed after acceptance"),
        "removing an accepted candidate rejects the whole construction"
    );
    // The prototype closure is part of the accepted selection, even when its content is equal.
    let mut replaced = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    replaced.prototypes = Catalog::from_current_selection(&snapshot, &keywords, &input)
        .unwrap()
        .prototypes;
    assert!(
        construct(replaced, "replaced").contains("current selection changed after acceptance"),
        "replacing the prototype catalogue rejects the whole construction"
    );
    // A current candidate cannot gain admission through an unrelated prototype catalogue.
    let accepted = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
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
    let changed = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card["number"] == NUMBER {
                card["faces"][0]["name"] = json!("foreign");
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let foreign = Catalog::from_current_selection(&changed, &keywords, &input).unwrap();
    assert!(
        construct(foreign, "foreign").contains("no unique snapshot source"),
        "a snapshot whose face text differs from the accepted source is rejected"
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
