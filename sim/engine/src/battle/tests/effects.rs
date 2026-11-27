use super::board_fixture::{Fixture, catalog_for, empty_board as board, fixture};
use super::board_fixtures::object;
use crate::battle::ProjectionError;
use crate::battle::effects::{project_card_effects, project_player_effects};
use crate::ir::{AbilityKind, CostKind, DamageKind};
use serde_json::Value;
use serde_json::json;
use sve_battle_contract::{MAX_SAFE_INTEGER, SourceRef, observation as wire};
use sve_engine::game::player::{Viewer, observation as native};

fn project(
    binding: &Fixture,
    observed: &native::Observation,
) -> Result<Vec<wire::InstalledEffect>, ProjectionError> {
    project_player_effects(&binding.catalog, &binding.references, observed, Viewer::P1)
}

const fn effect(change: native::InstalledChange) -> native::InstalledEffect {
    native::InstalledEffect {
        source: None,
        expires: native::EffectExpiry::TurnEnd,
        main_live: true,
        intervals: vec![],
        active: true,
        change,
    }
}

const fn cap(threshold: u64, replacement: u64) -> native::InstalledChange {
    native::InstalledChange::Prevention {
        kind: DamageKind::Any,
        change: native::PreventionChange::Cap {
            threshold,
            replacement,
        },
        remaining: None,
        recipients: None,
        sources: None,
    }
}

#[test]
fn wire_installed_effects_keep_frozen_operations_instead_of_effective_values() {
    use native::{InstalledChange as Change, NumberChange as Number};
    let cases = vec![
        (
            Change::Numeric {
                attack: Some(Number::Add(-2)),
                life: Some(Number::Set(9)),
            },
            json!({"kind":"numeric","attack":{"kind":"add","amount":-2_i32},"life":{"kind":"set","amount":9_i32}}),
        ),
        (
            Change::Numeric {
                attack: None,
                life: None,
            },
            json!({"kind":"numeric","attack":null,"life":null}),
        ),
        (
            Change::Cost {
                kind: CostKind::Play,
                change: Number::Set(0),
            },
            json!({"kind":"cost","cost":"play","change":{"kind":"set","amount":0_i32}}),
        ),
        (
            Change::Cost {
                kind: CostKind::Evolution,
                change: Number::Add(-1),
            },
            json!({"kind":"cost","cost":"evolution","change":{"kind":"add","amount":-1_i32}}),
        ),
        (
            Change::PlayerCost {
                change: Number::Add(2),
                limited: true,
            },
            json!({"kind":"player_cost","change":{"kind":"add","amount":2_i32},"limited":true}),
        ),
        (
            Change::PlayerCost {
                change: Number::Set(3),
                limited: false,
            },
            json!({"kind":"player_cost","change":{"kind":"set","amount":3_i32},"limited":false}),
        ),
        (
            cap(6, 5),
            json!({"kind":"prevention","damage":"any","change":{"kind":"cap","threshold":6_i32,"replacement":5_i32},"remaining":null,"recipients":null,"sources":null}),
        ),
    ];
    assert_changes(cases).unwrap();
}

fn assert_changes(cases: Vec<(native::InstalledChange, Value)>) -> Option<()> {
    let binding = fixture().ok()?;
    let mut observed = board(Viewer::P1, "unit-spell")
        .ok()?
        .observe(Viewer::P1)
        .ok()?;
    observed.players.get_mut(1)?.effects = vec![effect(native::InstalledChange::BanishPlay)];
    for (change, expected) in cases {
        observed.players.get_mut(0)?.effects = vec![effect(change.clone()), effect(change)];
        let projected = project(binding, &observed).ok()?;
        assert_eq!(projected.len(), 2, "both installed layers must be retained");
        let first = projected.first()?;
        assert_eq!(first, projected.get(1)?, "repeated layers must survive");
        assert_eq!(
            serde_json::to_value(first).ok()?.get("change"),
            Some(&expected),
            "frozen changes must use their exact wire fields and operands"
        );
        let mut reversed = observed.clone();
        reversed
            .players
            .get_mut(0)?
            .effects
            .insert(1, effect(native::InstalledChange::Pilot));
        let reordered = project(binding, &reversed).ok()?;
        assert_eq!(
            reordered.first(),
            reordered.get(2),
            "the outer repeated layers keep their positions"
        );
        assert_eq!(
            reordered.get(1)?.change,
            wire::InstalledChange::Pilot,
            "the middle layer cannot be sorted away"
        );
        let second =
            project_player_effects(&binding.catalog, &binding.references, &observed, Viewer::P2)
                .ok()?;
        assert!(
            matches!(second.as_slice(), [layer] if layer.change == wire::InstalledChange::BanishPlay),
            "selecting the second player cannot return the first player's layers"
        );
    }
    Some(())
}

#[test]
fn wire_installed_effects_preserve_grants_and_closed_restrictions() {
    use native::InstalledChange as Change;
    let mut cases = vec![
        (
            Change::Grant {
                keywords: vec!["ward".into(), "storm".into(), "ward".into()],
                traits: vec!["b".into(), "a".into()],
                definitions: vec![
                    native::GrantedDefinition {
                        kind: AbilityKind::Static,
                        line: 9,
                    },
                    native::GrantedDefinition {
                        kind: AbilityKind::Triggered,
                        line: 2,
                    },
                    native::GrantedDefinition {
                        kind: AbilityKind::Activated,
                        line: 3,
                    },
                    native::GrantedDefinition {
                        kind: AbilityKind::Spell,
                        line: 0,
                    },
                ],
            },
            json!({"kind":"grant","keywords":["ward","storm","ward"],"traits":["b","a"],"definitions":[{"ability":"static","line":9_i32},{"ability":"triggered","line":2_i32},{"ability":"activated","line":3_i32},{"ability":"spell","line":0_i32}]}),
        ),
        (Change::Pilot, json!({"kind":"pilot"})),
        (Change::BanishPlay, json!({"kind":"banish_play"})),
        (Change::NoLeaderAttack, json!({"kind":"no_leader_attack"})),
        (
            Change::AttackRestriction { leaders: None },
            json!({"kind":"attack_restriction","leaders":null}),
        ),
        (
            Change::AttackRestriction {
                leaders: Some(vec![]),
            },
            json!({"kind":"attack_restriction","leaders":[]}),
        ),
        (
            Change::AttackRestriction {
                leaders: Some(vec![Viewer::P2, Viewer::P1, Viewer::P2]),
            },
            json!({"kind":"attack_restriction","leaders":["p2","p1","p2"]}),
        ),
        (
            Change::NoStand { player: Viewer::P1 },
            json!({"kind":"no_stand","player":"p1"}),
        ),
        (
            Change::NoStand { player: Viewer::P2 },
            json!({"kind":"no_stand","player":"p2"}),
        ),
        (
            Change::Delayed {
                timing: native::TriggerTiming::Damage,
            },
            json!({"kind":"delayed","timing":"damage"}),
        ),
    ];
    for enabled in [false, true] {
        cases.push((
            Change::AbilityRemoval { boxed: enabled },
            json!({"kind":"ability_removal","boxed":enabled}),
        ));
        cases.push((
            Change::AbilityRestriction {
                except_evolution: enabled,
            },
            json!({"kind":"ability_restriction","except_evolution":enabled}),
        ));
    }
    assert_changes(cases).unwrap();
}

#[test]
fn wire_installed_effects_preserve_lifetime_and_interval_independently() {
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    for (expires, expected) in [
        (
            native::EffectExpiry::NoTemporalExpiry,
            json!({"kind":"no_temporal_expiry"}),
        ),
        (
            native::EffectExpiry::ResolutionEnd,
            json!({"kind":"resolution_end"}),
        ),
        (native::EffectExpiry::TurnEnd, json!({"kind":"turn_end"})),
        (
            native::EffectExpiry::SourceLeavesField,
            json!({"kind":"source_leaves_field"}),
        ),
        (
            native::EffectExpiry::NextTurnEnd(Viewer::P1),
            json!({"kind":"next_turn_end","player":"p1"}),
        ),
        (
            native::EffectExpiry::NextTurnEnd(Viewer::P2),
            json!({"kind":"next_turn_end","player":"p2"}),
        ),
        (
            native::EffectExpiry::NextTurnStart(Viewer::P1),
            json!({"kind":"next_turn_start","player":"p1"}),
        ),
        (
            native::EffectExpiry::NextTurnStart(Viewer::P2),
            json!({"kind":"next_turn_start","player":"p2"}),
        ),
        (
            native::EffectExpiry::NextStartEnd(Viewer::P1),
            json!({"kind":"next_start_end","player":"p1"}),
        ),
        (
            native::EffectExpiry::NextStartEnd(Viewer::P2),
            json!({"kind":"next_start_end","player":"p2"}),
        ),
    ] {
        for (main_live, active) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut layer = effect(native::InstalledChange::BanishPlay);
            layer.expires = expires;
            layer.main_live = main_live;
            layer.active = active;
            layer.intervals = vec![
                native::EffectInterval::NextTurn(Viewer::P2),
                native::EffectInterval::CurrentTurn,
                native::EffectInterval::NextTurn(Viewer::P1),
                native::EffectInterval::CurrentTurn,
            ];
            observed.players[0].effects = vec![layer];
            assert_eq!(
                serde_json::to_value(project(fixture().unwrap(), &observed).unwrap()).unwrap(),
                json!([{
                    "source":null,"expires":expected,"main_live":main_live,"active":active,
                    "intervals":[{"kind":"next_turn","player":"p2"},{"kind":"current_turn"},{"kind":"next_turn","player":"p1"},{"kind":"current_turn"}],
                    "change":{"kind":"banish_play"}
                }])
            );
        }
    }
}

#[test]
fn wire_installed_effects_keep_empty_restrictions_and_validate_every_current_link() {
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field).unwrap();
    for (kind, name) in [
        (DamageKind::Any, "any"),
        (DamageKind::Combat, "combat"),
        (DamageKind::Ability, "ability"),
    ] {
        for (change, description) in [
            (native::PreventionChange::Prevent, json!({"kind":"prevent"})),
            (
                native::PreventionChange::Add(-1),
                json!({"kind":"add","amount":-1_i32}),
            ),
        ] {
            observed.players[0].effects = vec![effect(native::InstalledChange::Prevention {
                kind,
                change,
                remaining: Some(0),
                recipients: Some(vec![
                    native::EffectRecipient::Card(holder.clone()),
                    native::EffectRecipient::Leader(Viewer::P2),
                    native::EffectRecipient::Card(holder.clone()),
                ]),
                sources: Some(vec![holder.clone(), holder.clone()]),
            })];
            assert_eq!(
                serde_json::to_value(&project(fixture().unwrap(), &observed).unwrap()[0].change)
                    .unwrap(),
                json!({
                    "kind":"prevention","damage":name,"change":description,"remaining":0_i32,
                    "recipients":[{"kind":"card","object":holder.as_str()},{"kind":"leader","player":"p2"},{"kind":"card","object":holder.as_str()}],
                    "sources":[holder.as_str(),holder.as_str()]
                })
            );
        }
    }
    let mut layer = effect(cap(4, 3));
    let native::InstalledChange::Prevention {
        recipients: empty_recipients,
        sources: empty_sources,
        ..
    } = &mut layer.change
    else {
        panic!("prevention")
    };
    *empty_recipients = Some(vec![]);
    *empty_sources = Some(vec![]);
    observed.players[0].effects = vec![layer];
    let result = serde_json::to_value(project(fixture().unwrap(), &observed).unwrap()).unwrap();
    assert_eq!(result[0]["change"]["recipients"], json!([]));
    assert_eq!(result[0]["change"]["sources"], json!([]));
    for bad_source in [false, true] {
        let mut retired = observed.clone();
        let native::InstalledChange::Prevention {
            recipients,
            sources,
            ..
        } = &mut retired.players[0].effects[0].change
        else {
            panic!("prevention")
        };
        if bad_source {
            *sources = Some(vec![holder.clone()]);
        } else {
            *recipients = Some(vec![native::EffectRecipient::Card(holder.clone())]);
        }
        for player in &mut retired.players {
            for zone in &mut player.zones {
                zone.cards.retain(|member| member.card.handle != holder);
            }
        }
        assert_eq!(
            project(fixture().unwrap(), &retired),
            Err(ProjectionError::Unavailable)
        );
    }
}

#[test]
fn wire_installed_effects_refuse_inexact_numbers_without_clamping_or_truncating() {
    use native::{InstalledChange as Change, NumberChange as Number};
    let limit = i64::try_from(MAX_SAFE_INTEGER).unwrap();
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    for amount in [-limit, limit] {
        observed.players[0].effects = vec![effect(Change::Numeric {
            attack: Some(Number::Add(amount)),
            life: Some(Number::Set(amount)),
        })];
        let result = serde_json::to_value(project(fixture().unwrap(), &observed).unwrap()).unwrap();
        assert_eq!(result[0]["change"]["attack"]["amount"], json!(amount));
        assert_eq!(result[0]["change"]["life"]["amount"], json!(amount));
    }
    for amount in [-limit - 1, limit + 1, i64::MIN, i64::MAX] {
        for change in [
            Change::Numeric {
                attack: Some(Number::Add(amount)),
                life: None,
            },
            Change::Numeric {
                attack: None,
                life: Some(Number::Set(amount)),
            },
            Change::Cost {
                kind: CostKind::Play,
                change: Number::Set(amount),
            },
            Change::PlayerCost {
                change: Number::Add(amount),
                limited: false,
            },
            Change::Prevention {
                kind: DamageKind::Any,
                change: native::PreventionChange::Add(amount),
                remaining: None,
                recipients: None,
                sources: None,
            },
        ] {
            observed.players[0].effects = vec![effect(change)];
            assert_eq!(
                project(fixture().unwrap(), &observed),
                Err(ProjectionError::Range)
            );
        }
    }
    for amount in [0, MAX_SAFE_INTEGER, MAX_SAFE_INTEGER + 1, u64::MAX] {
        for (change, pointer) in [
            (cap(amount, 0), "/change/change/threshold"),
            (cap(0, amount), "/change/change/replacement"),
            (
                Change::Grant {
                    keywords: vec![],
                    traits: vec![],
                    definitions: vec![native::GrantedDefinition {
                        kind: AbilityKind::Static,
                        line: amount,
                    }],
                },
                "/change/definitions/0/line",
            ),
            (
                Change::Prevention {
                    kind: DamageKind::Any,
                    change: native::PreventionChange::Prevent,
                    remaining: Some(amount),
                    recipients: None,
                    sources: None,
                },
                "/change/remaining",
            ),
        ] {
            observed.players[0].effects = vec![effect(change)];
            if amount > MAX_SAFE_INTEGER {
                assert_eq!(
                    project(fixture().unwrap(), &observed),
                    Err(ProjectionError::Range)
                );
            } else {
                let result =
                    serde_json::to_value(project(fixture().unwrap(), &observed).unwrap()).unwrap();
                assert_eq!(result[0].pointer(pointer), Some(&json!(amount)));
            }
        }
    }
}

#[test]
fn wire_installed_effects_select_only_authorized_card_state_and_origins() {
    let binding = fixture().unwrap();
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field).unwrap();
    let project_card = |observation: &native::Observation| {
        project_card_effects(&binding.catalog, &binding.references, observation, &holder)
    };
    assert_eq!(project_card(&observed), Ok(Some(vec![])));
    let mut layer = effect(native::InstalledChange::Pilot);
    layer.source = Some(native::EffectSource {
        provenance: native::TextProvenance::Unknown,
        card: "unit-follower".into(),
        face: Some(0),
        line: Some(2),
        wording: None,
        holder: Some(holder.clone()),
    });
    observed.players[0].effects = vec![layer.clone()];
    let SourceRef::Card {
        face: adopted_face,
        holder: link,
        ..
    } = &project(fixture().unwrap(), &observed).unwrap()[0]
        .source
        .clone()
        .unwrap()
    else {
        panic!("card origin")
    };
    assert_eq!(adopted_face.unwrap().get(), 1);
    assert_eq!(link.as_ref().unwrap().as_str(), holder.as_str());
    for player in &mut observed.players {
        for zone in &mut player.zones {
            for member in &mut zone.cards {
                if member.card.handle == holder {
                    member.card.properties.state.as_mut().unwrap().effects = vec![layer.clone()];
                }
            }
        }
    }
    assert_eq!(
        project_card(&observed).unwrap().unwrap(),
        project(fixture().unwrap(), &observed).unwrap()
    );
    for player in &mut observed.players {
        for zone in &mut player.zones {
            for member in &mut zone.cards {
                if member.card.handle == holder {
                    member.card.properties.state = None;
                }
            }
        }
    }
    assert_eq!(project_card(&observed), Ok(None));
    let wrong = catalog_for(&format!("{}\n", binding.snapshot)).unwrap();
    assert_eq!(
        project_player_effects(&wrong, &binding.references, &observed, Viewer::P1),
        Err(ProjectionError::Unavailable)
    );
    assert_eq!(
        project_card_effects(&wrong, &binding.references, &observed, &holder),
        Err(ProjectionError::Unavailable)
    );
    let known = "unit-follower";
    for (card, face, line, expected) in [
        ("missing", None, None, ProjectionError::Unavailable),
        (known, Some(99), None, ProjectionError::Unavailable),
        (
            known,
            Some(0),
            Some(MAX_SAFE_INTEGER + 1),
            ProjectionError::Range,
        ),
    ] {
        observed.players[0].effects[0].source = Some(native::EffectSource {
            provenance: native::TextProvenance::Unknown,
            card: card.into(),
            face,
            line,
            wording: None,
            holder: None,
        });
        assert_eq!(project(fixture().unwrap(), &observed), Err(expected));
    }
    let mut empty = observed.clone();
    empty.players[0].effects.clear();
    assert_eq!(
        project_player_effects(&wrong, &binding.references, &empty, Viewer::P1),
        Err(ProjectionError::Unavailable)
    );
    for player in &mut empty.players {
        for zone in &mut player.zones {
            zone.cards.retain(|member| member.card.handle != holder);
        }
    }
    assert_eq!(project_card(&empty), Err(ProjectionError::Unavailable));
}

#[test]
fn wire_installed_effects_retire_links_without_erasing_printing_and_refuse_duplicate_objects() {
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field).unwrap();
    let mut layer = effect(native::InstalledChange::Pilot);
    layer.source = Some(native::EffectSource {
        provenance: native::TextProvenance::Unknown,
        card: "unit-follower".into(),
        face: Some(0),
        line: Some(2),
        wording: None,
        holder: Some(holder.clone()),
    });
    observed.players[0].effects = vec![layer];
    let mut retired = observed.clone();
    for player in &mut retired.players {
        for zone in &mut player.zones {
            zone.cards.retain(|member| member.card.handle != holder);
        }
    }
    assert_eq!(
        project(fixture().unwrap(), &retired),
        Err(ProjectionError::Unavailable)
    );
    retired.players[0].effects[0]
        .source
        .as_mut()
        .unwrap()
        .holder = None;
    assert_eq!(
        serde_json::to_value(project(fixture().unwrap(), &retired).unwrap()).unwrap()[0]["source"],
        json!({
            "kind":"card","printing_id":"printing-follower","face":1_i32,"line":2_i32,"holder":null
        })
    );
    let field = observed.players[0]
        .zones
        .iter_mut()
        .find(|zone| zone.kind == native::ObservedZone::Field)
        .unwrap();
    let duplicate = field.cards[0].clone();
    field.cards.push(duplicate);
    assert_eq!(
        project(fixture().unwrap(), &observed),
        Err(ProjectionError::Unavailable)
    );
    assert_eq!(
        project_card_effects(
            &fixture().unwrap().catalog,
            &fixture().unwrap().references,
            &observed,
            &holder
        ),
        Err(ProjectionError::Unavailable)
    );
}
