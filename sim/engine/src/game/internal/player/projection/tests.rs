use super::*;
use crate::catalog::Catalog;
use crate::game::internal::{Context, DecisionToken, DecisionView};
use crate::game::player::{Command, Input, SubmissionKind};
use crate::ir::{Binding, DecisionId, ExecutionId, Generation, Program, ReceiptId, ScopeId};
use alloc::sync::Arc;
use serde_json::{Value, json};

fn game() -> Game {
    let snapshot = json!({"number":"unit","faces":[{"name":"unit","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string();
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{"unit":{"status":"complete","review":"synthetic","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let initial = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{"P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"s","card":"unit"}]}},"P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{}}}});
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "question-projection",
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
    let shape = typed(
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
    let choices = cards(vec![OrderCard {
        handle: 3,
        card: "printing".into(),
        name: "name".into(),
        object: Some("private-physical-object".into()),
    }]);
    assert_eq!(
        choices,
        vec![CardOption {
            handle: 3,
            card: "printing".into(),
            name: "name".into()
        }],
        "A player card choice carries only the occurrence coordinate and authorized print"
    );
    assert!(
        !format!("{choices:?}").contains("private-physical-object"),
        "Authority object identities cannot be represented by the DTO"
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
