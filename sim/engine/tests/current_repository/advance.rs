//! Synthetic whole cards prove runtime entry, not any official card's current-source qualification.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sve_engine::game::internal::{
    Activation, Answer, CardPlay, Fact, InputDomain, InstructionOutcome,
};
use sve_engine::game::{Game, View};
use sve_engine::ir::*;

const ADVANCE: &str = "synthetic-advance";
const FACE: &str = "f:00000000000000000000000000000001";
const CARD: &str = "c:00000000000000000000000000000001";

fn digest(text: &str) -> String {
    // sha2 0.11 digests do not implement LowerHex.
    const HEX: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        result.push(char::from(HEX[usize::from(byte >> 4_u8)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

fn handoff() -> Value {
    let text = "Synthetic fanfare: deal 1 damage to all leaders.";
    let source = json!({"binding_id":"synthetic-binding","ability_scope":"synthetic-fanfare",
        "occurrence":{"owner":{"kind":"face_revision","revision_id":"synthetic-revision"},
        "field":"effect","ordinal":null,"line_ordinal":0_u8,"source_hash":digest(text),"role":"body","segments":[{"start":0_u8,"end":1_u8}]}});
    let body = json!({"fanfare":{"do":[{"damage":{"to":{"leaders":"all"},"n":1_u8}}]},
        "ports":{"imports":{},"exports":{}}});
    let mut ability = body.clone();
    ability["line"] = json!(1_u8);
    ability["sources"] = json!([source]);
    let version = format!("effect-source-v1:sha256:{}", digest("synthetic-only"));
    json!({"tree":{"format":1_u8,"kind":"author_effect_set","scope":"shared","cards":{
        ADVANCE:{"meta":{"dsl":"1.0","source":version,"written_by":[{"actor":{"who":"synthetic fixture","model":null},"date":"2026-10-26"}],"reviewed_by":[],"status":"draft","verified_by_exam":[],"rulings":[]},"abilities":[ability]}}},
        "card_id":CARD,"card_no":ADVANCE,"source_version":version,
        "source_payload":{"recipe":"effect-source-v1","region":"jp","token_links":[],
            "faces":[{"face_id":FACE,"revision_id":"synthetic-revision",
                "attributes":{"cost":{"state":"known","value":1_u8},
                    "attack":{"state":"known","value":2_u8},"defense":{"state":"known","value":3_u8}},
                "texts":[{"field":"name","ordinal":null,"state":"present","source_hash":digest(ADVANCE)},
                    {"field":"effect","ordinal":null,"state":"present","source_hash":digest(text)}]}]},
        "source_inventory":{"abilities":[{"source_use":source,"body":body}],"non_body":[]},
        "declaration_sidecar":[],
        "sidecar":[{"path":["cards",ADVANCE,"abilities",0_u8],"source_proof":"current_source_checked",
            "source_use":source,"input_hash":format!("sha256:{}",digest("synthetic-input")),
            "source_input_hash":format!("sha256:{}",digest(text))}]})
}

fn selector(zone: ZoneKind) -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            text_quantifier: TextQuantifier::All,
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
        },
        zone,
        view: SelectionView::Current {},
        binding: Binding::new(50, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn opposing_field() -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            text_quantifier: TextQuantifier::All,
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
        },
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(50, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn entry(zone: ZoneKind) -> Action {
    Action::Move {
        objects: selector(zone),
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

fn initial(seat: &str, zone: &str, owner: &str) -> Value {
    let mut initial = runtime::setup(seat, seat, [20, 20]);
    let other = if seat == "P1" { "P2" } else { "P1" };
    initial["players"][seat]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    initial["players"][seat]["zones"][zone] =
        json!([{"id":"incoming","card":ADVANCE,"owner":owner}]);
    initial["players"][other]["zones"]["field"] = json!([
        {"id":"returning","card":ADVANCE,"owner":seat},
        {"id":"token","card":"synthetic-token","owner":seat}
    ]);
    initial
}

fn game(initial: &Value, action: Action, kind: &str) -> Game {
    game_steps(initial, vec![action], kind)
}

fn game_steps(initial: &Value, actions: Vec<Action>, kind: &str) -> Game {
    let input = handoff();
    let text = "Synthetic fanfare: deal 1 damage to all leaders.";
    let snapshot = [
        (ADVANCE, kind, text),
        ("producer", "フォロワー", ""),
        ("synthetic-token", "フォロワー・トークン", ""),
    ]
    .map(|(number, kind, text)| {
        json!({
            "number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,
            "cost":"1","power":"2","hp":"3","traits":[],"text":text,"sections":[]}]
        })
        .to_string()
    })
    .join("\n");
    let selection = json!({"documents":[["synthetic.yaml",
        "format: 1\nkind: effect_set\ncards:\n  producer:\n    status: complete\n    abilities: []\n  synthetic-token:\n    status: complete\n    abilities: []\n"]],
        "authors":[{"status":"accepted","input":input}]});
    let keywords = "format: 1\nkind: keyword_registry\nkeywords: {}\n";
    let selected = Catalog::from_current_selection(&snapshot, keywords, &selection).unwrap();
    assert_eq!(
        selected.current[ADVANCE].card_id, CARD,
        "the accepted handoff keeps the permanent card id"
    );
    let evidence = selected.current[ADVANCE]
        .source_at(&format!("/cards/{ADVANCE}/abilities/0"))
        .unwrap();
    assert_eq!(
        evidence.source.face, FACE,
        "the source holder keeps the permanent face id"
    );
    assert_eq!(
        evidence.source.owner,
        json!({"kind":"face_revision","revision_id":"synthetic-revision"}).to_string(),
        "the source holder keeps the revision owner"
    );
    let mut game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "synthetic-advance",
    )
    .unwrap();
    game.load_internal_card("producer", &runtime::producer_steps(actions))
        .unwrap();
    game.load_internal_card(
        "synthetic-token",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn views(game: &Game) -> [Value; 3] {
    [View::P1, View::P2, View::Referee].map(|v| game.projection(v).unwrap())
}

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: if seat == "P1" { 1 } else { 2 },
    }
}

#[expect(
    clippy::panic,
    reason = "These synthetic abilities have no decisions inside their bodies."
)]
fn resolved(play: CardPlay) -> Activation {
    if let CardPlay::Resolved(result) = play {
        result
    } else {
        panic!("expected resolved synthetic ability")
    }
}

#[test]
fn synthetic_current_advance_enters_by_effect_and_fanfare_uses_after_entry_holder() {
    // CR 9.2.2 and 12.4; official FAQ Q1363-Q1365 distinguish entry from play.
    for seat in ["P1", "P2"] {
        for (zone, kind) in [("evolve_deck", ZoneKind::Evolve), ("ex", ZoneKind::Ex)] {
            let mut game = game(
                &initial(seat, zone, seat),
                entry(kind),
                "フォロワー・アドバンス",
            );
            let entered = resolved(
                game.activate_internal_card("producer", AbilityId(1), actor(seat))
                    .unwrap(),
            );
            assert!(
                !entered
                    .progress
                    .facts
                    .iter()
                    .any(|f| matches!(f, Fact::CardPlayed { .. } | Fact::CardResolved { .. })),
                "effect entry creates no card play"
            );
            for state in views(&game) {
                assert_eq!(
                    state["objects"]["incoming"]["card"], ADVANCE,
                    "current card identity survives entry"
                );
                assert_eq!(
                    state["objects"]["incoming"]["generation"], 1_u8,
                    "entry creates one new generation"
                );
                assert_eq!(
                    state[seat]["pp"]["current"], 10_u8,
                    "putting a card is not paying its play cost"
                );
            }
            game.resume_internal_card().unwrap();
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = q.domain else {
                panic!("pending fanfare")
            };
            assert_eq!(options.len(), 1, "only the incoming holder triggers");
            assert_eq!(
                options[0].card, ADVANCE,
                "the complete current card owns its fanfare"
            );
            let result = resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
            assert_eq!(
                result.source.holder.generation,
                Generation(1),
                "fanfare uses the after-entry holder"
            );
            assert_eq!(
                result.source.controller,
                actor(seat),
                "fanfare belongs to the entering controller"
            );
            for state in views(&game) {
                for leader in ["P1", "P2"] {
                    assert_eq!(
                        state[leader]["leader"]["life"], 19_u8,
                        "the fanfare damaged every leader"
                    );
                }
            }
        }
    }
}

#[test]
fn destroyed_advance_returns_after_cemetery_and_token_is_erased() {
    // CR 9.2.2 and 9.1.4.4: both rule moves happen before the destroying effect continues.
    for seat in ["P1", "P2"] {
        let mut game = game(
            &initial(seat, "evolve_deck", seat),
            Action::Destroy {
                objects: opposing_field(),
            },
            "フォロワー・アドバンス",
        );
        let destroyed = resolved(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        assert!(
            matches!(destroyed.progress.receipts.get(&ReceiptId(1)),
                Some(InstructionOutcome::Destroy(outcome))
                    if outcome.successful.len() == 2
                        && outcome.successful.iter().all(|member|
                            member.before.is_some() && member.after.is_none())),
            "both destructions succeed, but immediate rules leave no current cemetery endpoint"
        );
        let moves = destroyed
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Movement(m) = fact {
                    Some(m)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(moves.len(), 2, "both opposing special cards are destroyed");
        for fact in &moves {
            assert_eq!(
                fact.movement.requested_to,
                RuntimeZoneRef {
                    player: actor(seat),
                    kind: ZoneKind::Cemetery
                },
                "destruction requests the owner's cemetery"
            );
            assert_eq!(
                fact.movement.after.unwrap().generation,
                Generation(fact.movement.before.generation.0 + 1),
                "the receipt keeps the first cemetery generation"
            );
        }
        for state in views(&game) {
            assert!(
                state[seat]["evolve_deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("returning")),
                "Advance returns immediately to its owner"
            );
            assert_eq!(
                state["objects"]["returning"]["generation"], 2_u8,
                "cemetery and rule return are separate moves"
            );
            assert_eq!(
                state["objects"]["returning"]["face_up"], true,
                "the returned Advance is public"
            );
            assert!(
                !state[seat]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("token")),
                "token is erased before continuation"
            );
        }
    }
}

#[test]
fn following_a_special_destroy_receipt_never_reuses_erased_or_returned_endpoints() {
    // The destroyed token and the returned Advance are no longer at their cemetery endpoints.
    for seat in ["P1", "P2"] {
        let follow = Action::Move {
            objects: Selector::MoveAfter {
                receipt: MovementReceipt::Destroy {
                    receipt: ReceiptReference::new(ReceiptId(1)),
                },
                permission: FollowPermission::SameEffectExplicitReference {},
            },
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        };
        let mut game = game_steps(
            &initial(seat, "evolve_deck", seat),
            vec![
                Action::Destroy {
                    objects: opposing_field(),
                },
                follow,
            ],
            "フォロワー・アドバンス",
        );
        let result = resolved(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        let moved = result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Movement(_)))
            .count();
        assert_eq!(moved, 2, "the follow instruction moves nothing");
        for state in views(&game) {
            assert!(
                state[seat]["evolve_deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("returning")),
                "the returned Advance stays in its owner's evolve deck"
            );
            assert!(
                !state[seat]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("token")),
                "the erased token is not a live object"
            );
        }
    }
}

#[test]
fn invalid_advance_entry_and_ordinary_play_are_atomic() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        let mut cases = vec![
            (initial(seat, "evolve_deck", other), ZoneKind::Evolve),
            (initial(seat, "cemetery", seat), ZoneKind::Cemetery),
        ];
        let mut full = initial(seat, "evolve_deck", seat);
        for n in 0_u8..4_u8 {
            full["players"][seat]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("filler{n}"),"card":"producer"}));
        }
        cases.push((full, ZoneKind::Evolve));
        let mut wrong_time = initial(seat, "evolve_deck", seat);
        wrong_time["turn"]["phase"] = json!("end");
        cases.push((wrong_time, ZoneKind::Evolve));
        for (initial, zone) in cases {
            let mut game = game(&initial, entry(zone), "フォロワー・アドバンス");
            let before = views(&game);
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap_err();
            assert_eq!(
                views(&game),
                before,
                "illegal effect entry is atomic in every view"
            );
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "rejected entry leaves no fanfare"
            );
        }
        let mut game = game(
            &initial(seat, "evolve_deck", seat),
            entry(ZoneKind::Evolve),
            "フォロワー・アドバンス",
        );
        let before = views(&game);
        game.play_internal_card("incoming", actor(seat))
            .unwrap_err();
        game.activate_internal_card("incoming", AbilityId(1), actor(seat))
            .unwrap_err();
        game.activate_internal_card("producer", AbilityId(1), actor(other))
            .unwrap_err();
        assert_eq!(
            views(&game),
            before,
            "neither direct play nor an invented AdvanceActivate bypasses the effect"
        );
    }
}
