#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::{SearchAnswer, TargetRoleAssignment};

pub(super) const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

pub(super) fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"their-draw","card":"BP01-171"}]);
    initial
}

pub(super) fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "admission-bp-cp",
    )
    .unwrap()
}

pub(super) fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                let targets = d
                    .members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect::<Vec<_>>();
                if d.roles.is_empty() {
                    Answer::Targets(targets)
                } else {
                    let roles = d
                        .roles
                        .iter()
                        .map(|role| TargetRoleAssignment {
                            role: role.role,
                            targets: match role.cardinality {
                                RoleCardinality::Fixed { cardinality } => targets
                                    .iter()
                                    .take(usize::try_from(cardinality.minimum.0).unwrap())
                                    .copied()
                                    .collect(),
                                RoleCardinality::IfNonempty { .. } => {
                                    targets.iter().take(1).copied().collect()
                                }
                                RoleCardinality::Remainder {} => {
                                    targets.iter().skip(1).copied().collect()
                                }
                            },
                        })
                        .collect();
                    Answer::TargetsWithRoles { targets, roles }
                }
            }
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
            InputDomain::ResourceEntryPayment { materials, .. } => Answer::ResourceEntryPayment {
                materials: materials
                    .members
                    .iter()
                    .take(
                        usize::try_from(
                            materials
                                .cardinality
                                .maximum
                                .unwrap_or(materials.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
                use_ep: false,
            },
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Integer(bounds) => {
                Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum))
            }
            InputDomain::DiceReroll { .. } => panic!("unexpected dice decision"),
            InputDomain::Abilities(_)
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Modes { .. }
            | InputDomain::TokenCapacity { .. } => panic!("Unexpected card decision"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

pub(super) fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("Expected a pending ability")
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, play);
    }
}

pub(super) fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    settle(game);
}

pub(super) fn activation(card: &str) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(
                    a.node.source.origin,
                    AbilityOrigin::Printed { .. }
                        | AbilityOrigin::Keyword {
                            keyword: KeywordId::AdvanceActivate,
                            ..
                        }
                )
        })
        .unwrap()
        .node
        .id
}

pub(super) fn activate(game: &mut Game, card: &str) {
    let step = game
        .activate_internal_card("source", activation(card), actor())
        .unwrap();
    finish(game, step);
    settle(game);
}

pub(super) fn evolve(board: &mut Game, form: &str, pp: u8) {
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":form,"pay":{"pp":pp}})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the printed evolution must resolve before its triggered ability"
    );
    settle(board);
}

pub(super) fn next_main(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(board);
    while let Some(q) = board.internal_quick_choice().unwrap() {
        let step = board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(board, step);
        settle(board);
    }
    settle(board);
}

pub(super) fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}

pub(super) fn destruction_game(initial: &Value) -> Game {
    with_producer(
        initial,
        Action::Destroy {
            objects: Selector::Difference {
                input: Box::new(Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(0),
                            maximum: None,
                        },
                        text_quantifier: TextQuantifier::All,
                    },
                    zone: ZoneKind::Field,
                    view: SelectionView::Current {},
                    binding: Binding::new(1, ScopeId(1)),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                }),
                except: Box::new(Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }),
            },
        },
    )
}
