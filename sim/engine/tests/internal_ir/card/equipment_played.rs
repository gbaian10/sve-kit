#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate equipment Play scenarios."
)]
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

pub(super) fn tree(head: &Value, number: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry
        .as_object_mut()
        .unwrap()
        .extend(head.as_object().unwrap().clone());
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({number:{"meta":meta,"abilities":[entry]}});
    tree
}

fn definition() -> Value {
    json!({"auto":{"per_turn":2,"on":{"played":{"who":"self","sel":{"kind":"spell"}}},"do":[{"damage":{"to":{"leaders":"opponents"},"n":{"half_up":{"power_of":"this"}}}}]}})
}

fn equipment() -> WellFormedCardProgram {
    lowerer().lower(&tree(&json!({"static":{"zone":"equipment","effects":[{"grant":{"on":"host","abilities":[definition()]}}]}}),"unit-amulet")).unwrap().remove("unit-amulet").unwrap()
}

fn initial(seat: u32, equipment: usize) -> Value {
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(format!("P{seat}"));
    for player in ["P1", "P2"] {
        initial["players"][player]["pp"] = json!({"current":10,"max":10});
        initial["players"][player]["zones"]["field"] = json!([]);
        initial["players"][player]["zones"]["hand"] = json!([]);
        initial["players"][player]["zones"]["deck"] =
            json!([{"id":format!("deck-{player}"),"card":"unit-follower"}]);
    }
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"host","card":"unit-follower","state":{"power":3}}]);
    initial["players"]["P1"]["zones"]["equipment"] = json!((0..equipment).map(|i|json!({"id":format!("eq{i}"),"card":"unit-amulet","state":{"equipped_to":"host"}})).collect::<Vec<_>>());
    for i in 0..3 {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("spell{i}"),"card":"unit-spell"}));
    }
    initial
}

fn game(initial: &Value, steps: Vec<Located<Node>>) -> Game {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("イクイップメント・トークン");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut game = Game::new(
        catalog,
        initial,
        &Value::Null,
        &Value::Null,
        "equipment-play",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &equipment())
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(steps)]))
        .unwrap();
    game
}

fn resolve_pending(game: &mut Game) -> usize {
    let mut resolved_count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = question.domain else {
            panic!("pending Play");
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        resolved_count = resolved_count.checked_add(1).unwrap();
        assert!(resolved_count <= 4, "unexpected Play trigger loop");
    }
    resolved_count
}

fn play_spell(game: &mut Game, id: &str, seat: u32) -> usize {
    resolved(game.play_internal_card(id, PlayerId { seat }).unwrap());
    game.resume_internal_card().unwrap();
    resolve_pending(game)
}

#[test]
fn equipment_granted_play_uses_host_power_and_each_providers_own_two_use_limit() {
    let mut game = game(&initial(1, 2), vec![]);
    assert_eq!(play_spell(&mut game, "spell0", 1), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        16
    );
    assert_eq!(play_spell(&mut game, "spell1", 1), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        12
    );
    assert_eq!(play_spell(&mut game, "spell2", 1), 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        12
    );
}

#[test]
fn equipment_granted_play_does_not_observe_an_opponents_spell_or_an_unpaid_play() {
    let mut opposing = initial(2, 1);
    opposing["players"]["P2"]["zones"]["hand"] = json!([{"id":"opposing","card":"unit-spell"}]);
    let mut game_opposing = game(&opposing, vec![]);
    assert_eq!(play_spell(&mut game_opposing, "opposing", 2), 0);
    let mut unpaid = initial(1, 1);
    unpaid["players"]["P1"]["pp"]["current"] = json!(0);
    let mut game_unpaid = game(&unpaid, vec![]);
    let before = exact_authority(&game_unpaid);
    game_unpaid
        .play_internal_card("spell0", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(exact_authority(&game_unpaid), before);
    assert!(game_unpaid.internal_pending_choice().unwrap().is_none());
}

#[test]
fn equipment_spell_subscription_does_not_match_a_played_follower() {
    let mut initial = initial(1, 1);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"follower","card":"unit-follower"}]);
    let mut game = game(&initial, vec![]);
    assert!(matches!(
        game.play_internal_card("follower", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn a_silenced_host_or_equipment_does_not_supply_the_granted_play_trigger() {
    for silenced in ["host", "equipment"] {
        let mut initial = initial(1, 1);
        let zone = if silenced == "host" {
            "field"
        } else {
            "equipment"
        };
        initial["players"]["P1"]["zones"][zone][0]["state"]["silenced"] = json!(true);
        let mut game = game(&initial, vec![]);
        assert_eq!(play_spell(&mut game, "spell0", 1), 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20
        );
    }
}

#[test]
fn a_trigger_captured_before_the_host_leaves_uses_the_hosts_last_known_power() {
    let move_host = instruction(
        1,
        Action::Move {
            objects: zone(ZoneKind::Field, 77),
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
    );
    let mut game = game(&initial(1, 1), vec![move_host]);
    assert_eq!(play_spell(&mut game, "spell0", 1), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!([]));
    assert_eq!(view["P1"]["equipment"], json!([]));
    assert_eq!(view["P2"]["leader"]["life"], 18);
}

#[test]
fn played_definitions_still_require_an_equipment_provider() {
    for head in [
        json!({"static":{"effects":[{"grant":{"on":"this","abilities":[definition()]}}]}}),
        json!({"activate":{"cost":{"pp":0},"do":[{"grant":{"on":"this","abilities":[definition()]}}]}}),
    ] {
        let mut game = game(&initial(1, 1), vec![]);
        let program = lowerer()
            .lower(&tree(&head, "unit-follower"))
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        assert!(game.load_internal_card("unit-follower", &program).is_err());
    }
}
