use super::*;
use crate::catalog::Catalog;
use crate::game::internal::{Context, DecisionToken, DecisionView, OrderCard};
use crate::game::player::{CardOption, Command, Input, SubmissionKind};
use crate::ir::{Binding, DecisionId, ExecutionId, Generation, Program, ReceiptId, ScopeId};
use alloc::sync::Arc;
use serde_json::{Value, json};

fn game_in(zone: &str) -> Game {
    let snapshot = json!({"number":"unit","faces":[{"name":"unit","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string();
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{"unit":{"status":"complete","review":"synthetic","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{"P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"s","card":"unit"}]}},"P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{}}}});
    if zone != "field" {
        let cards = initial["players"]["P1"]["zones"]["field"].take();
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"][zone] = cards;
    }
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "question-projection",
    )
    .unwrap()
}

fn game() -> Game {
    game_in("field")
}

fn execution(game: &mut Game) -> Execution {
    let identities = game.internal_identities().unwrap();
    game.card_execution(
        Context {
            source: "s".into(),
            generation: Generation(0),
            controller: Viewer::P1.player(),
        },
        identities,
    )
    .unwrap()
}

fn view(domain: InputDomain) -> DecisionView {
    DecisionView {
        token: DecisionToken {
            execution: ExecutionId(0),
            decision: DecisionId(0),
            epoch: 0,
        },
        actor: PlayerId { seat: 1 },
        domain,
    }
}

fn bounds() -> Cardinality {
    Cardinality {
        minimum: Nat(0),
        maximum: Some(Nat(1)),
    }
}

fn assert_persistent_fault(mut game: Game) {
    game.advance_player_input();
    for actor in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            game.input(actor),
            Input::EngineFault,
            "A broken cached projection stops either viewer"
        );
    }
    let before = format!("{game:?}");
    game.advance_player_input();
    assert_eq!(
        format!("{game:?}"),
        before,
        "Reentering handoff cannot resume the corrupt question"
    );
    assert_eq!(
        game.submit_player(Viewer::P1, Command::Concede).kind,
        SubmissionKind::Applied,
        "Projection faults do not block concession"
    );
}

#[test]
fn player_projection_sparse_route_indices_are_not_renumbered_or_filled() {
    let mut game = game();
    let execution = execution(&mut game);
    let shape = typed(
        &game,
        &execution,
        Viewer::P1,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2)),
        }),
        &Wait::ExchangeRoute {
            affordable: vec![0, 2],
        },
    )
    .unwrap();
    assert_eq!(
        shape,
        DecisionShape::ExchangeRoutes(vec![Nat(0), Nat(2)]),
        "Route handles are the native affordable indices, including their gaps"
    );
}

#[test]
fn player_projection_card_lists_have_no_authority_object_field() {
    let mut game = game();
    let execution = execution(&mut game);
    let reference = execution.reference(&game, "s").unwrap();
    let links = super::super::CandidateLinks::capture(&game, &execution, [reference]).unwrap();
    let choices = game
        .player_cards(
            Viewer::P1,
            &[OrderCard {
                handle: 3,
                card: "printing".into(),
                name: "name".into(),
                object: Some("private-physical-object".into()),
            }],
            &[Some(reference)],
            &links,
        )
        .unwrap();
    let expected = game.state.knowledge["P1"].handle("s").unwrap();
    assert_eq!(
        choices,
        vec![CardOption {
            handle: 3,
            card: "printing".into(),
            name: "name".into(),
            object: Some(expected),
            rules_names: None,
            original_cost: None,
        }]
    );
    assert!(
        !format!("{choices:?}").contains("private-physical-object"),
        "Authority identities cannot be represented by the DTO"
    );
}

#[test]
fn player_projection_waiting_viewer_does_not_build_the_private_shape() {
    let mut game = game();
    let identities = game.internal_identities().unwrap();
    let mut execution = game
        .card_execution(
            Context {
                source: "s".into(),
                generation: Generation(0),
                controller: Viewer::P1.player(),
            },
            identities,
        )
        .unwrap();
    execution.waiting = Some((
        view(InputDomain::BeforeDraw),
        Wait::May {
            body: Program {
                scope: ScopeId(1),
                steps: vec![],
            },
            binding: Binding::new(1, ScopeId(1)),
            receipt: ReceiptId(1),
        },
    ));
    game.typed = Some(execution);
    let before = format!("{game:?}");
    assert!(
        matches!(game.input(Viewer::P2), Input::Waiting(waiting) if waiting.actor == Viewer::P1 && waiting.dice.is_none()),
        "The nonactor does not evaluate a private malformed shape"
    );
    assert_eq!(
        format!("{game:?}"),
        before,
        "Reading a waiting summary does not change authority"
    );
    assert_persistent_fault(game);
}

#[test]
fn player_projection_pending_and_quota_caches_are_checked_at_handoff() {
    for quota in [false, true] {
        let mut game = game();
        let mut question = if quota {
            view(InputDomain::TriggerQuota(
                super::super::super::QuotaDomain {
                    candidates: vec![],
                    earlier: vec![],
                },
            ))
        } else {
            view(InputDomain::Integer(bounds()))
        };
        if quota {
            question.actor = PlayerId { seat: 3 };
        }
        game.card_runtime.question = Some((question, vec![]));
        assert_persistent_fault(game);
    }
}

#[test]
fn player_projection_quick_cache_is_checked_at_handoff() {
    let mut game = game();
    game.state.flow = json!({"kind":"end","stage":"quick"});
    game.internal_quick_choice().unwrap().unwrap();
    game.card_runtime.quick.as_mut().unwrap().view.domain = InputDomain::Integer(bounds());
    assert_persistent_fault(game);
}

#[test]
fn player_projection_candidate_links_do_not_rebind_after_shuffle_and_relearning() {
    use super::super::CandidateLinks;
    let mut game = game_in("deck");
    let execution = execution(&mut game);
    let reference = execution.reference(&game, "s").unwrap();
    let unknown = CandidateLinks::capture(&game, &execution, [reference]).unwrap();
    assert_eq!(
        unknown
            .observed(&game, Viewer::P1, Some(reference))
            .unwrap(),
        None
    );
    game.learn("P1", "s", true).unwrap();
    let original = game.state.knowledge["P1"].handle("s").unwrap();
    let links = CandidateLinks::capture(&game, &execution, [reference]).unwrap();
    assert_eq!(
        links.observed(&game, Viewer::P1, Some(reference)).unwrap(),
        Some(original.clone())
    );
    assert_eq!(
        links.observed(&game, Viewer::P2, Some(reference)).unwrap(),
        None
    );
    assert_eq!(
        unknown
            .observed(&game, Viewer::P1, Some(reference))
            .unwrap(),
        None,
        "A candidate unknown at capture must not gain a link later"
    );
    game.shuffle("P1", &["s".into()]).unwrap();
    assert_eq!(game.object("s").unwrap().generation, reference.generation.0);
    assert_eq!(
        links.observed(&game, Viewer::P1, Some(reference)).unwrap(),
        None
    );
    game.learn("P1", "s", true).unwrap();
    let replacement = game.state.knowledge["P1"].handle("s").unwrap();
    assert_ne!(replacement, original);
    let before = format!("{game:?}");
    for _ in 0..3_u8 {
        assert_eq!(
            links.observed(&game, Viewer::P1, Some(reference)).unwrap(),
            None,
            "The new Observation period cannot identify an old frozen candidate"
        );
    }
    assert_eq!(before, format!("{game:?}"));
    assert_eq!(
        CandidateLinks::capture(&game, &execution, [reference])
            .unwrap()
            .observed(&game, Viewer::P1, Some(reference))
            .unwrap(),
        Some(replacement)
    );
}

#[test]
fn player_projection_look_continuation_preserves_the_original_identification_period() {
    use super::super::CandidateLinks;
    use crate::ir::{
        Bool, Expr, ExtremumKind, MemberProjection, NumericProperty, ObjectReceipt,
        ReceiptReference, Selector,
    };
    let mut game = game_in("deck");
    let mut execution = execution(&mut game);
    let reference = execution.reference(&game, "s").unwrap();
    game.learn("P1", "s", true).unwrap();
    let links = CandidateLinks::capture(&game, &execution, [reference]).unwrap();
    execution.player_receipt_links.insert(ReceiptId(1), links);
    let receipt = Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        projection: MemberProjection::Before,
    };
    let selector = Selector::Filter {
        input: Box::new(receipt),
        binding: Binding::new(2, ScopeId(0)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let original = game.state.knowledge["P1"].handle("s").unwrap();
    let current =
        CandidateLinks::for_selectors(&game, &execution, [reference], [&selector]).unwrap();
    assert_eq!(
        current
            .observed(&game, Viewer::P1, Some(reference))
            .unwrap(),
        Some(original.clone())
    );
    game.shuffle("P1", &["s".into()]).unwrap();
    game.learn("P1", "s", true).unwrap();
    let replacement = game.state.knowledge["P1"].handle("s").unwrap();
    assert_ne!(replacement, original);
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .players
            .iter()
            .flat_map(|player| &player.zones)
            .flat_map(|zone| &zone.cards)
            .any(|card| card.card.handle == replacement)
    );
    // Target, Order and capacity Waits use this same source walk, without consumer classification.
    for continuation in [
        selector.clone(),
        Selector::Union {
            sets: vec![selector.clone()],
        },
        Selector::Difference {
            input: Box::new(selector.clone()),
            except: Box::new(Selector::Objects { objects: vec![] }),
        },
        Selector::Extremum {
            input: Box::new(selector.clone()),
            property: NumericProperty::OriginalCost,
            kind: ExtremumKind::Minimum,
        },
    ] {
        let continued_links =
            CandidateLinks::for_selectors(&game, &execution, [reference], [&continuation]).unwrap();
        assert_eq!(
            continued_links
                .observed(&game, Viewer::P1, Some(reference))
                .unwrap(),
            None
        );
    }
    execution
        .player_receipt_links
        .insert(ReceiptId(1), CandidateLinks::default());
    assert!(
        CandidateLinks::for_selectors(&game, &execution, [reference], [&selector]).is_err(),
        "A missing Look member cannot inherit the new period"
    );
    execution.player_receipt_links.clear();
    assert!(
        CandidateLinks::for_selectors(&game, &execution, [reference], [&selector]).is_err(),
        "Missing Look provenance must not fall back to the current period"
    );
}
