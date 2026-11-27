use super::*;
use crate::ir::PlayerId;
use serde_json::Value;

#[test]
fn proposed_damage_controller_requires_an_explicit_valid_seat() {
    for hit in [
        json!({}),
        json!({"controller":null}),
        json!({"controller":1_u8}),
        json!({"controller":"P3"}),
    ] {
        opposing_damage_controller(&hit, PlayerId { seat: 1 }).unwrap_err();
    }
    assert!(
        !opposing_damage_controller(&json!({"controller":"P1"}), PlayerId { seat: 1 }).unwrap()
    );
    assert!(opposing_damage_controller(&json!({"controller":"P2"}), PlayerId { seat: 1 }).unwrap());
}

use crate::catalog::Catalog;
use crate::ir::*;
use alloc::sync::Arc;

fn catalog() -> Arc<Catalog> {
    let snapshot = [json!({"number":"shield","faces":[{"name":"shield","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}), json!({"number":"source","faces":[{"name":"source","card_class":"ニュートラル","card_type":"スペル","cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]})].map(|v|v.to_string()).join("\n");
    let registry = json!({"format":1_u8,"kind":"keyword_registry","rule_names":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"fixture_cards","cards":{"shield":{"status":"complete"},"source":{"status":"complete"}}}).to_string();
    Arc::new(
        Catalog::from_fixture_documents(
            &snapshot,
            &registry,
            &[("synthetic.yaml".into(), document)],
        )
        .unwrap(),
    )
}

fn board(filtered: bool) -> Game {
    board_with_source(
        filtered.then(|| prevention::opponent_controller(Binding::new(1, ScopeId(3)))),
    )
}

fn board_with_source(source_filter: Option<SourcePredicate>) -> Game {
    let catalog = catalog();
    let initial = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{"P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"zones":{"hand":[{"id":"s","card":"source"}]}},"P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"f","card":"shield"}]}}}});
    let mut board = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "legacy-native-prevention",
    )
    .unwrap();
    let span = SourceSpan {
        card_id: "shield".into(),
        card_no: "shield".into(),
        scope: "synthetic".into(),
        owner: "card".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: None,
        author_path: "/".into(),
    };
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
        controller: PlayerRef::Controller {},
    };
    let binding = Binding::new(1, ScopeId(2));
    let declaration = Effect::Replacement {
        id: EffectDefinitionId(1),
        source: origin.clone(),
        pattern: ProposedEventPattern::Damage {
            recipient: Some(RecipientPredicate {
                binding,
                condition: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
                    recipient: DamageRecipient::Bound { binding },
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                })),
            }),
            source: source_filter,
            kind: DamageKind::Ability,
        },
        condition: Expr::Literal(Bool(true)),
        rewrite: EventRewrite::PreventDamage {
            recipient: None,
            source: None,
            kind: DamageKind::Any,
        },
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedDamageRecipient {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
    };
    let program = WellFormedCardProgram::check(CardProgram {
        card_play_costs: vec![],
        abilities: vec![Located {
            source: span,
            node: Ability {
                id: AbilityId(1),
                scope: ScopeId(1),
                kind: AbilityKind::Static,
                qualification: AbilityQualification::Ordinary,
                source: origin,
                active_zones: vec![ZoneKind::Field],
                event_captures: vec![],
                play: None,
                trigger: None,
                declarations: vec![declaration],
                body: None,
            },
        }],
    })
    .unwrap();
    board.load_internal_card("shield", &program).unwrap();
    board
        .load_internal_card(
            "source",
            &WellFormedCardProgram::check(CardProgram {
                card_play_costs: vec![],
                abilities: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    board
}

#[test]
fn missing_proposed_controller_is_rejected_only_by_filtered_ability_prevention() {
    for filtered in [false, true] {
        let board = board(filtered);
        for malformed in [
            None,
            Some(json!(null)),
            Some(json!(1_u8)),
            Some(json!("P3")),
        ] {
            let mut hit = json!({"source":"s","source_generation":0_u8,"target":"f","amount":2_u8,"battle":false});
            if let Some(value) = malformed {
                hit["controller"] = value;
            }
            assert_eq!(
                board
                    .static_prevention_references(&hit, None, &mut vec![])
                    .is_err(),
                filtered
            );
            hit["battle"] = json!(true);
            let mut references = vec![];
            board
                .static_prevention_references(&hit, None, &mut references)
                .unwrap();
            assert!(references.is_empty());
        }
    }
}
