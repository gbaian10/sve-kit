//! Read-only resolution between fixture labels and one seat's capabilities,
//! and the exam-vocabulary rendering of the current serial question.

use serde_json::{Value, json};
use sve_scenario_runner::EngineError;

use super::{adapter, seat};
use crate::game::Game;
use crate::game::player::{
    AttackTarget, CardOption, ClosingQuestion, DecisionShape, MainIntentKind, ObjectHandle,
    PendingChoice, Question, QuestionShape, SelectionQuestion, Viewer,
};
use crate::ir::{Nat, Position};

/// Read-only fixture label resolution for one seat's capabilities.
#[derive(Clone, Copy)]
pub(super) struct Labels<'game> {
    pub(super) game: &'game Game,
    pub(super) by: Viewer,
}

impl Labels<'_> {
    pub(super) fn label(&self, handle: &ObjectHandle) -> Option<String> {
        self.game.referee_label(self.by, handle)
    }

    pub(super) fn members(
        &self,
        question: &SelectionQuestion,
        wanted: &[String],
    ) -> Result<Vec<u64>, EngineError> {
        wanted
            .iter()
            .map(|label| {
                question
                    .members
                    .iter()
                    .find(|member| {
                        label.strip_suffix(".leader").map_or_else(
                            || {
                                member
                                    .object
                                    .as_ref()
                                    .and_then(|handle| self.label(handle))
                                    .as_deref()
                                    == Some(label.as_str())
                            },
                            |player| {
                                member.card.is_none()
                                    && seat(&json!(player)).ok() == Some(member.player)
                            },
                        )
                    })
                    .map(|member| member.handle)
                    .ok_or_else(|| adapter(format!("{label} is not a candidate")))
            })
            .collect()
    }

    pub(super) fn cards(
        &self,
        cards: &[CardOption],
        wanted: &[String],
    ) -> Result<Vec<u64>, EngineError> {
        wanted
            .iter()
            .map(|label| {
                cards
                    .iter()
                    .find(|card| {
                        card.object
                            .as_ref()
                            .and_then(|handle| self.label(handle))
                            .as_deref()
                            == Some(label.as_str())
                    })
                    .map(|card| card.handle)
                    .ok_or_else(|| adapter(format!("{label} is not a card candidate")))
            })
            .collect()
    }

    pub(super) fn handles<'observed>(
        &self,
        observed: impl Iterator<Item = &'observed ObjectHandle>,
        wanted: &[String],
    ) -> Result<Vec<ObjectHandle>, EngineError> {
        let observed: Vec<&ObjectHandle> = observed.collect();
        wanted
            .iter()
            .map(|label| {
                observed
                    .iter()
                    .find(|handle| self.label(handle).as_deref() == Some(label.as_str()))
                    .map(|handle| (*handle).clone())
                    .ok_or_else(|| adapter(format!("{label} is not offered")))
            })
            .collect()
    }

    /// The occurrence's holder label: the seat's own capability while it can still
    /// identify the holder, otherwise the referee's record of the frozen occurrence.
    pub(super) fn pending_source(&self, choice: &PendingChoice) -> Option<String> {
        choice
            .source
            .as_ref()
            .and_then(|text| text.holder.as_ref())
            .and_then(|handle| self.label(handle))
            .or_else(|| {
                self.game
                    .referee_pending_holder(choice.handle)
                    .map(str::to_owned)
            })
    }

    /// Fields each exam verb answers in this native question; later questions' fields
    /// are not among them. The runner cuts written choices to exactly these.
    pub(super) fn fields(question: &Question) -> Value {
        match &question.shape {
            QuestionShape::Decision(shape) => match shape {
                DecisionShape::Pending(_) => json!({"choose-pending": ["pending"]}),
                DecisionShape::Targets(selection) if !selection.roles.is_empty() => {
                    json!({"resolve-choice": ["select", "position"]})
                }
                DecisionShape::Targets(_)
                | DecisionShape::Discard { .. }
                | DecisionShape::Search(_)
                | DecisionShape::SearchPartition(_)
                | DecisionShape::MovementCapacity(_) => json!({"resolve-choice": ["select"]}),
                DecisionShape::CostMaterials(_) => json!({"resolve-choice": ["costs"]}),
                DecisionShape::May
                | DecisionShape::OptionalCost
                | DecisionShape::ResolutionPayment
                | DecisionShape::DiceReroll { .. } => json!({"resolve-choice": ["choice"]}),
                DecisionShape::DriveTrigger { .. } => {
                    json!({"resolve-choice": ["choice", "select"]})
                }
                DecisionShape::Modes { .. } | DecisionShape::ResolutionAlternative { .. } => {
                    json!({"resolve-choice": ["options"]})
                }
                DecisionShape::Order(_) => json!({"resolve-choice": ["order"]}),
                DecisionShape::Quick { .. } => json!({"play": ["card"], "activate": ["ability"]}),
                DecisionShape::Abilities(_)
                | DecisionShape::ResourceEntryPayment { .. }
                | DecisionShape::WardEntry { .. }
                | DecisionShape::ExchangeRoutes(_)
                | DecisionShape::BeforeDraw
                | DecisionShape::TriggerQuota(_)
                | DecisionShape::Integer(_)
                | DecisionShape::DamageReplacements(_)
                | DecisionShape::Distribution(_)
                | DecisionShape::SearchReplacement
                | DecisionShape::TokenCapacity(_) => json!({}),
            },
            QuestionShape::ActionWindow(_) => {
                json!({"play": ["card"], "activate": ["ability"], "evolve": ["source"]})
            }
            QuestionShape::Evolution(_) | QuestionShape::Opening(_) | QuestionShape::Closing(_) => {
                json!({})
            }
        }
    }

    /// One question picks the cards and the deck edge they go to; every subset is
    /// offered once per edge role. Roles without an edge have no exam rendering.
    fn role_choices(&self, selection: &SelectionQuestion) -> Option<Vec<Value>> {
        let edges = selection
            .roles
            .iter()
            .map(|role| match role.position {
                Some(Position::Top {}) => Some("top"),
                Some(Position::Bottom {}) => Some("bottom"),
                Some(Position::Index { .. }) | None => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let mut result = Vec::new();
        for subset in subsets(
            &self.member_labels(selection),
            selection.cardinality.minimum.0,
            selection.cardinality.maximum,
        )? {
            for edge in &edges {
                result.push(json!({"do": "resolve-choice", "select": subset, "position": edge}));
            }
        }
        Some(result)
    }

    fn member_labels(&self, question: &SelectionQuestion) -> Vec<String> {
        question
            .members
            .iter()
            .map(|member| {
                member
                    .object
                    .as_ref()
                    .and_then(|handle| self.label(handle))
                    .unwrap_or_else(|| format!("{:?}.leader", member.player))
            })
            .collect()
    }

    fn card_labels(&self, cards: &[CardOption]) -> Vec<String> {
        cards
            .iter()
            .map(|card| {
                card.object
                    .as_ref()
                    .and_then(|handle| self.label(handle))
                    .unwrap_or_else(|| card.card.clone())
            })
            .collect()
    }

    fn selects(
        items: &[String],
        minimum: u64,
        maximum: Option<Nat>,
        verb: &str,
        field: &str,
    ) -> Option<Vec<Value>> {
        Some(
            subsets(items, minimum, maximum)?
                .into_iter()
                .map(|subset| {
                    if field == "costs" {
                        json!({"do": verb, "costs": {"1": subset}})
                    } else {
                        json!({"do": verb, field: subset})
                    }
                })
                .collect(),
        )
    }

    /// The current serial question in exam vocabulary, when it has a direct rendering.
    #[expect(
        clippy::too_many_lines,
        reason = "One closed rendering of typed question shapes in exam vocabulary."
    )]
    pub(super) fn choices(&self, question: &Question) -> Option<Vec<Value>> {
        let execute = || {
            Some(vec![
                json!({"do": "resolve-choice", "choice": "execute"}),
                json!({"do": "resolve-choice", "choice": "decline"}),
            ])
        };
        match &question.shape {
            QuestionShape::Decision(shape) => match shape {
                DecisionShape::Pending(choices) => Some(
                    choices
                        .iter()
                        .map(|choice| {
                            let holder = self.pending_source(choice);
                            json!({"do": "choose-pending",
                                "pending": {"ability": {"source": holder, "line": choice.line}}})
                        })
                        .collect(),
                ),
                DecisionShape::Targets(selection) if !selection.roles.is_empty() => {
                    self.role_choices(selection)
                }
                DecisionShape::Targets(selection)
                | DecisionShape::Discard {
                    cards: selection, ..
                } => Self::selects(
                    &self.member_labels(selection),
                    selection.cardinality.minimum.0,
                    selection.cardinality.maximum,
                    "resolve-choice",
                    "select",
                ),
                DecisionShape::CostMaterials(selection) => Self::selects(
                    &self.member_labels(selection),
                    selection.cardinality.minimum.0,
                    selection.cardinality.maximum,
                    "resolve-choice",
                    "costs",
                ),
                DecisionShape::Search(search) => Self::selects(
                    &self.card_labels(&search.cards),
                    search.cardinality.minimum.0,
                    search.cardinality.maximum,
                    "resolve-choice",
                    "select",
                ),
                DecisionShape::SearchPartition(partition) => Self::selects(
                    &self.card_labels(&partition.cards),
                    partition.cardinality.minimum.0,
                    partition.cardinality.maximum,
                    "resolve-choice",
                    "select",
                ),
                DecisionShape::May
                | DecisionShape::OptionalCost
                | DecisionShape::ResolutionPayment
                | DecisionShape::DiceReroll { .. } => execute(),
                DecisionShape::DriveTrigger { targets, .. } => {
                    let mut result: Vec<Value> = self
                        .card_labels(targets)
                        .into_iter()
                        .map(|target| {
                            json!({"do": "resolve-choice", "choice": "execute", "select": [target]})
                        })
                        .collect();
                    if result.is_empty() {
                        result.push(json!({"do": "resolve-choice", "choice": "execute"}));
                    }
                    result.push(json!({"do": "resolve-choice", "choice": "decline"}));
                    Some(result)
                }
                DecisionShape::Modes { modes, count, .. }
                | DecisionShape::ResolutionAlternative { modes, count, .. } => {
                    let ids: Vec<String> = modes.iter().map(|mode| mode.0.to_string()).collect();
                    Some(
                        subsets(&ids, count.0, Some(*count))?
                            .into_iter()
                            .map(|subset| {
                                let options: Vec<u64> =
                                    subset.iter().filter_map(|id| id.parse().ok()).collect();
                                json!({"do": "resolve-choice", "options": options})
                            })
                            .collect(),
                    )
                }
                DecisionShape::Quick { options, .. } => {
                    let mut result = vec![json!({"do": "pass"})];
                    for option in options {
                        let label = option.object.as_ref().and_then(|handle| self.label(handle));
                        result.push(option.line.map_or_else(
                            || json!({"do": "play", "card": label}),
                            |line| json!({"do": "activate", "ability": {"source": label, "line": line}}),
                        ));
                    }
                    Some(result)
                }
                DecisionShape::WardEntry { object, .. } => {
                    let label = object.as_ref().and_then(|handle| self.label(handle));
                    Some(vec![
                        json!({"do": "place-acted", "object": label, "acted": true}),
                        json!({"do": "place-acted", "object": label, "acted": false}),
                    ])
                }
                DecisionShape::DamageReplacements(choices) => {
                    let items: Vec<Value> = choices
                        .iter()
                        .map(|choice| {
                            let label =
                                choice.object.as_ref().and_then(|handle| self.label(handle));
                            json!({"source": label, "line": choice.line})
                        })
                        .collect();
                    Some(
                        permutations(&items)?
                            .into_iter()
                            .map(|order| json!({"do": "order-replacements", "order": order}))
                            .collect(),
                    )
                }
                DecisionShape::Order(cards) => {
                    let items: Vec<Value> = self
                        .card_labels(cards)
                        .into_iter()
                        .map(Value::String)
                        .collect();
                    Some(
                        permutations(&items)?
                            .into_iter()
                            .map(|order| json!({"do": "resolve-choice", "order": order}))
                            .collect(),
                    )
                }
                DecisionShape::Abilities(_)
                | DecisionShape::ResourceEntryPayment { .. }
                | DecisionShape::ExchangeRoutes(_)
                | DecisionShape::BeforeDraw
                | DecisionShape::TriggerQuota(_)
                | DecisionShape::Integer(_)
                | DecisionShape::MovementCapacity(_)
                | DecisionShape::Distribution(_)
                | DecisionShape::SearchReplacement
                | DecisionShape::TokenCapacity(_) => None,
            },
            QuestionShape::Closing(ClosingQuestion::GuardAct(cards)) => {
                let labels: Vec<String> = cards
                    .iter()
                    .filter_map(|card| self.label(&card.handle))
                    .collect();
                Self::selects(&labels, 0, None, "guard-act", "select")
            }
            QuestionShape::ActionWindow(actions) => {
                let mut result = Vec::new();
                for intent in &actions.intents {
                    match &intent.kind {
                        MainIntentKind::EndPhase => result.push(json!({"do": "end-phase"})),
                        MainIntentKind::Play { card } => {
                            result.push(json!({"do": "play", "card": self.label(&card.handle)}));
                        }
                        MainIntentKind::Activate { holder, line, .. } => result.push(json!({
                            "do": "activate",
                            "ability": {"source": self.label(&holder.handle), "line": line}
                        })),
                        MainIntentKind::Evolve { holder, .. } => result
                            .push(json!({"do": "evolve", "source": self.label(&holder.handle)})),
                        MainIntentKind::Attack { attacker, targets } => {
                            for target in targets {
                                let target = match target {
                                    AttackTarget::Leader(player) => format!("{player:?}.leader"),
                                    AttackTarget::Card(handle) => {
                                        self.label(handle).unwrap_or_default()
                                    }
                                };
                                result.push(json!({"do": "attack",
                                    "attacker": self.label(&attacker.handle), "target": target}));
                            }
                        }
                    }
                }
                Some(result)
            }
            QuestionShape::Closing(ClosingQuestion::Discard { .. })
            | QuestionShape::Evolution(_)
            | QuestionShape::Opening(_) => None,
        }
    }
}

/// Every subset within the bounds, for small candidate sets only.
fn subsets(items: &[String], minimum: u64, maximum: Option<Nat>) -> Option<Vec<Vec<String>>> {
    if items.len() > 10 {
        return None;
    }
    let maximum = maximum.map_or_else(
        || u64::try_from(items.len()).unwrap_or(u64::MAX),
        |max| max.0,
    );
    let mut result = Vec::new();
    for mask in 0_u32..(1_u32 << items.len()) {
        let size = u64::from(mask.count_ones());
        if size < minimum || size > maximum {
            continue;
        }
        result.push(
            items
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, item)| item.clone())
                .collect(),
        );
    }
    Some(result)
}

fn permutations(items: &[Value]) -> Option<Vec<Vec<Value>>> {
    if items.len() > 4 {
        return None;
    }
    if items.len() <= 1 {
        return Some(vec![items.to_vec()]);
    }
    let mut result = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for mut tail in permutations(&rest)? {
            tail.insert(0, item.clone());
            result.push(tail);
        }
    }
    Some(result)
}
