//! Seat actions: each exam decision becomes player commands from that seat's current question.

#![expect(
    clippy::indexing_slicing,
    reason = "Exam decisions are read through serde_json's total indexing; a missing key is null."
)]

use serde_json::{Value, json};
use sve_scenario_runner::EngineError;

use super::labels::Labels;
use super::params::{Params, labels};
use super::{adapter, fault, seat, shape_name, untranslated};
use crate::game::Game;
use crate::game::internal::{DamageShare, SearchAnswer};
use crate::game::player::{
    Answer, AttackTarget, CardOption, ClosingAnswer, ClosingQuestion, Command, DecisionShape,
    DiscardKind, EvolutionAnswer, EvolutionQuestion, Input, MainActionAnswer, MainIntent,
    MainIntentKind, ObjectHandle, OpeningAnswer, OpeningQuestion, PendingChoice, Question,
    QuestionPurpose, QuestionShape, QuickChoice, SelectionGroup, SelectionQuestion, SubmissionKind,
    TargetRoleAssignment, Viewer,
};
use crate::ir::{ModeId, Nat, Position, StepwiseAnswer};
use core::mem::take;

/// Serial player questions one exam decision may span, including forced Quick passes.
const MAX_QUESTIONS: usize = 64;

pub(super) struct Turn<'game> {
    pub(super) game: &'game mut Game,
    pub(super) by: Viewer,
}

impl Turn<'_> {
    fn question(&self) -> Result<Option<Question>, EngineError> {
        match self.game.input(self.by) {
            Input::Question(question) => Ok(Some(*question)),
            Input::EngineFault => Err(fault(&*self.game)),
            Input::Waiting(_) | Input::Finished { .. } => Ok(None),
        }
    }

    fn require(&self) -> Result<Question, EngineError> {
        self.question()?
            .ok_or_else(|| adapter(format!("{:?} has no question", self.by)))
    }

    fn submit(&mut self, command: Command) -> Result<SubmissionKind, EngineError> {
        let submission = self.game.submit_player(self.by, command);
        if submission.kind == SubmissionKind::EngineFault {
            return Err(fault(&*self.game));
        }
        Ok(submission.kind)
    }

    fn answer(
        &mut self,
        question: &Question,
        answer: Answer,
    ) -> Result<SubmissionKind, EngineError> {
        self.submit(Command::Answer {
            token: question.token,
            answer,
        })
    }

    const fn labels(&self) -> Labels<'_> {
        Labels {
            game: &*self.game,
            by: self.by,
        }
    }

    fn label(&self, handle: &ObjectHandle) -> Option<String> {
        self.labels().label(handle)
    }

    fn members(
        &self,
        question: &SelectionQuestion,
        wanted: &[String],
    ) -> Result<Vec<u64>, EngineError> {
        self.labels().members(question, wanted)
    }

    fn cards(&self, cards: &[CardOption], wanted: &[String]) -> Result<Vec<u64>, EngineError> {
        self.labels().cards(cards, wanted)
    }

    fn handles<'observed>(
        &self,
        observed: impl Iterator<Item = &'observed ObjectHandle>,
        wanted: &[String],
    ) -> Result<Vec<ObjectHandle>, EngineError> {
        self.labels().handles(observed, wanted)
    }

    /// Answers one decision-shaped question from the exam parameters, if they cover it.
    #[expect(
        clippy::too_many_lines,
        reason = "One closed mapping from typed question shapes to exam parameters."
    )]
    fn parameter(
        &mut self,
        question: &Question,
        params: &mut Params,
    ) -> Result<Option<SubmissionKind>, EngineError> {
        let QuestionShape::Decision(shape) = &question.shape else {
            return Ok(None);
        };
        let answer = match shape {
            DecisionShape::Modes { .. } | DecisionShape::ResolutionAlternative { .. } => {
                let Some(options) = params.options.take() else {
                    return Ok(None);
                };
                Answer::Modes(options.into_iter().map(ModeId).collect())
            }
            DecisionShape::Targets(selection) => {
                let wanted = if !params.targets.is_empty() {
                    params.targets.remove(0)
                } else if let Some(select) = params.select.take() {
                    select
                } else if params.choice == Some(false) && selection.cardinality.minimum.0 == 0 {
                    // Declining an optional choice is choosing nobody.
                    params.choice = None;
                    Vec::new()
                } else {
                    return Ok(None);
                };
                let targets = self.members(selection, &wanted)?;
                if selection.roles.is_empty() {
                    Answer::Targets(targets)
                } else {
                    let edge = match params.position.take().as_deref() {
                        Some("top") => Position::Top {},
                        Some("bottom") => Position::Bottom {},
                        _ => return Ok(None),
                    };
                    let roles = selection
                        .roles
                        .iter()
                        .map(|role| TargetRoleAssignment {
                            role: role.role,
                            targets: if role.position == Some(edge) {
                                targets.clone()
                            } else {
                                Vec::new()
                            },
                        })
                        .collect();
                    Answer::TargetsWithRoles { targets, roles }
                }
            }
            DecisionShape::Distribution(distribution) => {
                if params.distribute.is_empty() {
                    return Ok(None);
                }
                let shares = params.distribute.remove(0);
                let mut answer = Vec::new();
                for member in &distribution.members {
                    let label = member
                        .object
                        .as_ref()
                        .and_then(|handle| self.label(handle))
                        .unwrap_or_else(|| format!("{:?}.leader", member.player));
                    let amount = shares[label.as_str()].as_u64().unwrap_or_default();
                    answer.push(DamageShare {
                        handle: member.handle,
                        amount: Nat(amount),
                    });
                }
                Answer::Distribution(answer)
            }
            DecisionShape::CostMaterials(selection) => {
                let wanted = if params.costs.is_empty() {
                    let Some(select) = params.select.take() else {
                        return Ok(None);
                    };
                    select
                } else {
                    params.costs.remove(0)
                };
                Answer::CostMaterials(self.members(selection, &wanted)?)
            }
            DecisionShape::Discard { cards, kind } => {
                let wanted = match kind {
                    DiscardKind::Cost if !params.costs.is_empty() => params.costs.remove(0),
                    DiscardKind::Cost | DiscardKind::Effect => match params.select.take() {
                        Some(select) => select,
                        None if !params.costs.is_empty() => params.costs.remove(0),
                        None => return Ok(None),
                    },
                };
                let handles = self.members(cards, &wanted)?;
                if matches!(kind, DiscardKind::Cost) {
                    Answer::CostMaterials(handles)
                } else {
                    Answer::Targets(handles)
                }
            }
            DecisionShape::OptionalCost | DecisionShape::ResolutionPayment => {
                if !params.optional.is_empty() {
                    Answer::OptionalCost(params.optional.remove(0))
                } else if let Some(choice) = params.choice.take() {
                    Answer::OptionalCost(choice)
                } else if !params.costs.is_empty() {
                    Answer::OptionalCost(true)
                } else {
                    return Ok(None);
                }
            }
            DecisionShape::May | DecisionShape::DiceReroll { .. } => {
                let Some(choice) = params.choice.take() else {
                    return Ok(None);
                };
                Answer::Integer(Nat(u64::from(choice)))
            }
            DecisionShape::BeforeDraw => {
                let Some(proceed) = params.proceed.take().or_else(|| params.choice.take()) else {
                    return Ok(None);
                };
                Answer::Stepwise(if proceed {
                    StepwiseAnswer::Continue
                } else {
                    StepwiseAnswer::Stop
                })
            }
            DecisionShape::Integer(_) => {
                let Some(x) = params.x.take() else {
                    return Ok(None);
                };
                Answer::Integer(Nat(x))
            }
            DecisionShape::Search(search) => {
                let Some(select) = params.select.take() else {
                    return Ok(None);
                };
                let chosen = self.cards(&search.cards, &select)?;
                match &search.groups {
                    None => Answer::Search(SearchAnswer::Ungrouped(chosen)),
                    Some(groups) => {
                        Answer::Search(SearchAnswer::Grouped(unique_grouping(groups, &chosen)?))
                    }
                }
            }
            DecisionShape::SearchPartition(partition) => {
                let Some(select) = params.select.take().or_else(|| params.order.take()) else {
                    return Ok(None);
                };
                Answer::SearchPartition(self.cards(&partition.cards, &select)?)
            }
            DecisionShape::MovementCapacity(capacity) => {
                let Some(select) = params.select.take() else {
                    return Ok(None);
                };
                Answer::SearchPartition(self.cards(&capacity.selection.cards, &select)?)
            }
            DecisionShape::Order(cards) => {
                let Some(order) = params.order.take() else {
                    return Ok(None);
                };
                Answer::Order(self.cards(cards, &order)?)
            }
            DecisionShape::DriveTrigger { targets, .. } => {
                let accept = params.choice.take().unwrap_or(true);
                let target = match params.select.take() {
                    Some(select) if !select.is_empty() => {
                        self.cards(targets, &select)?.first().copied()
                    }
                    _ => None,
                };
                Answer::DriveTrigger { accept, target }
            }
            DecisionShape::ResourceEntryPayment { materials, .. } => {
                let wanted = if params.costs.is_empty() {
                    Vec::new()
                } else {
                    params.costs.remove(0)
                };
                Answer::ResourceEntryPayment {
                    materials: self.members(materials, &wanted)?,
                    use_ep: params.pay["ep"].as_u64().unwrap_or_default() > 0,
                }
            }
            DecisionShape::Abilities(_)
            | DecisionShape::WardEntry { .. }
            | DecisionShape::Quick { .. }
            | DecisionShape::ExchangeRoutes(_)
            | DecisionShape::Pending(_)
            | DecisionShape::TriggerQuota(_)
            | DecisionShape::DamageReplacements(_)
            | DecisionShape::SearchReplacement
            | DecisionShape::TokenCapacity(_) => return Ok(None),
        };
        self.answer(question, answer).map(Some)
    }

    /// Answer serial questions of this decision until its parameters stop covering them.
    fn follow(
        &mut self,
        params: &mut Params,
        mut kind: SubmissionKind,
    ) -> Result<SubmissionKind, EngineError> {
        for _ in 0..MAX_QUESTIONS {
            if kind != SubmissionKind::Applied {
                return Ok(kind);
            }
            let Some(question) = self.question()? else {
                return Ok(kind);
            };
            if matches!(
                question.purpose,
                QuestionPurpose::QuickWindow | QuestionPurpose::MainAction
            ) || params.is_empty()
            {
                return Ok(kind);
            }
            if let QuestionShape::Evolution(evolution) = &question.shape {
                kind = self.evolution(&question, evolution, params)?;
                continue;
            }
            match self.parameter(&question, params)? {
                Some(next) => kind = next,
                None => return Ok(kind),
            }
        }
        Err(adapter("decision spans too many questions"))
    }

    fn evolution(
        &mut self,
        question: &Question,
        evolution: &EvolutionQuestion,
        params: &mut Params,
    ) -> Result<SubmissionKind, EngineError> {
        let answer = match evolution {
            EvolutionQuestion::Form(forms) => {
                let wanted = params
                    .select
                    .take()
                    .and_then(|select| select.first().cloned())
                    .ok_or_else(|| adapter("evolution form not given"))?;
                let face = params.x.take();
                let form = forms
                    .iter()
                    .find(|form| {
                        self.label(&form.card.handle).as_deref() == Some(wanted.as_str())
                            && face.is_none_or(|face| form.face == face)
                    })
                    .ok_or_else(|| adapter(format!("{wanted} is not an evolution form")))?;
                EvolutionAnswer::Form(form.id)
            }
            EvolutionQuestion::Materials(selection) | EvolutionQuestion::Discard(selection) => {
                let wanted = if params.costs.is_empty() {
                    Vec::new()
                } else {
                    params.costs.remove(0)
                };
                let handles = self.members(selection, &wanted)?;
                if matches!(evolution, EvolutionQuestion::Materials(_)) {
                    EvolutionAnswer::Materials(handles)
                } else {
                    EvolutionAnswer::Discard(handles)
                }
            }
            EvolutionQuestion::Payment(_) => {
                let pay = take(&mut params.pay);
                EvolutionAnswer::Payment {
                    pp: pay["pp"].as_i64().unwrap_or_default(),
                    ep: pay["ep"].as_u64().unwrap_or_default() > 0,
                    super_evolution: pay["sep"].as_u64().unwrap_or_default() > 0,
                }
            }
        };
        self.submit(Command::Evolution {
            token: question.token,
            answer,
        })
    }
}

/// A question that stops inside a card or ability resolution, as the exam's `paused`.
const fn mid_resolution(question: &Question) -> bool {
    match &question.shape {
        QuestionShape::Evolution(_) => true,
        QuestionShape::Decision(shape) => !matches!(
            shape,
            DecisionShape::Pending(_)
                | DecisionShape::TriggerQuota(_)
                | DecisionShape::Quick { .. }
        ),
        QuestionShape::Opening(_) | QuestionShape::Closing(_) | QuestionShape::ActionWindow(_) => {
            false
        }
    }
}

fn intent(
    question: &Question,
    mut accept: impl FnMut(&MainIntentKind) -> bool,
) -> Option<&MainIntent> {
    let QuestionShape::ActionWindow(actions) = &question.shape else {
        return None;
    };
    actions.intents.iter().find(|intent| accept(&intent.kind))
}

/// A Quick play or activation, answered from the current Quick window.
fn quick_action(
    turn: &mut Turn<'_>,
    verb: &str,
    decision: &Value,
    question: &Question,
    options: &[QuickChoice],
    params: &mut Params,
) -> Result<Option<SubmissionKind>, EngineError> {
    let (label, line) = if verb == "play" {
        (decision["card"].as_str().unwrap_or_default(), None)
    } else {
        (
            decision["ability"]["source"].as_str().unwrap_or_default(),
            decision["ability"]["line"].as_u64(),
        )
    };
    let Some(choice) = options.iter().find(|choice| {
        choice
            .object
            .as_ref()
            .and_then(|handle| turn.label(handle))
            .as_deref()
            == Some(label)
            && line.is_none_or(|line| choice.line == Some(line))
    }) else {
        return Ok(None);
    };
    let kind = turn.answer(question, Answer::Quick(Some(choice.handle)))?;
    turn.follow(params, kind).map(Some)
}

/// A main-phase intent and its serial declaration parameters; `None` when not offered.
fn main_action(
    turn: &mut Turn<'_>,
    verb: &str,
    decision: &Value,
    question: &Question,
    params: &mut Params,
) -> Result<Option<SubmissionKind>, EngineError> {
    let kind = match verb {
        "play" => {
            let card = decision["card"].as_str().unwrap_or_default();
            let Some(found) = intent(
                question,
                |kind| matches!(kind, MainIntentKind::Play { card: c } if turn.label(&c.handle).as_deref() == Some(card)),
            ) else {
                return Ok(None);
            };
            let kind = turn.submit(Command::MainAction {
                token: question.token,
                intent: found.id.clone(),
                answer: MainActionAnswer::None,
            })?;
            turn.follow(params, kind)?
        }
        "activate" => {
            let source = decision["ability"]["source"].as_str().unwrap_or_default();
            let line = decision["ability"]["line"].as_u64();
            let card = decision["ability"]["card"].as_str();
            let Some(found) = intent(question, |kind| {
                matches!(kind, MainIntentKind::Activate { holder, source: text, line: l }
                    if turn.label(&holder.handle).as_deref() == Some(source)
                        && line.is_none_or(|line| *l == line)
                        && card.is_none_or(|card| text.as_ref().is_none_or(|text| text.card == card)))
            }) else {
                return Ok(None);
            };
            let kind = turn.submit(Command::MainAction {
                token: question.token,
                intent: found.id.clone(),
                answer: MainActionAnswer::None,
            })?;
            turn.follow(params, kind)?
        }
        "evolve" => {
            let source = decision["source"].as_str().unwrap_or_default();
            let line = decision["ability"]["line"].as_u64();
            let Some(found) = intent(question, |kind| {
                matches!(kind, MainIntentKind::Evolve { holder, line: l }
                    if turn.label(&holder.handle).as_deref() == Some(source)
                        && line.is_none_or(|line| *l == line))
            }) else {
                return Ok(None);
            };
            params.select = decision["evolve_card"]
                .as_str()
                .map(|card| vec![card.to_owned()]);
            params.x = decision["face"].as_u64();
            let kind = turn.submit(Command::MainAction {
                token: question.token,
                intent: found.id.clone(),
                answer: MainActionAnswer::None,
            })?;
            turn.follow(params, kind)?
        }
        "attack" => {
            let attacker = decision["attacker"].as_str().unwrap_or_default();
            let target = decision["target"].as_str().unwrap_or_default();
            let Some(found) = intent(question, |kind| {
                matches!(kind, MainIntentKind::Attack { attacker: a, .. }
                    if turn.label(&a.handle).as_deref() == Some(attacker))
            }) else {
                return Ok(None);
            };
            let MainIntentKind::Attack { targets, .. } = &found.kind else {
                return Err(adapter("attack intent changed shape"));
            };
            let Some(recipient) = targets.iter().find(|recipient| match recipient {
                AttackTarget::Leader(player) => {
                    target
                        .strip_suffix(".leader")
                        .and_then(|p| seat(&json!(p)).ok())
                        == Some(*player)
                }
                AttackTarget::Card(handle) => turn.label(handle).as_deref() == Some(target),
            }) else {
                return Ok(None);
            };
            turn.submit(Command::MainAction {
                token: question.token,
                intent: found.id.clone(),
                answer: MainActionAnswer::Attack(recipient.clone()),
            })?
        }
        "end-phase" => {
            let Some(found) = intent(question, |kind| matches!(kind, MainIntentKind::EndPhase))
            else {
                return Err(adapter("end-phase is not offered"));
            };
            turn.submit(Command::MainAction {
                token: question.token,
                intent: found.id.clone(),
                answer: MainActionAnswer::None,
            })?
        }
        _ => return Err(untranslated(format!("{verb} at ActionWindow"))),
    };
    Ok(Some(kind))
}

/// The exam's pending occurrence, which must be the only candidate it can name.
fn choose_pending(
    turn: &mut Turn<'_>,
    pending: &Value,
    question: &Question,
    choices: &[PendingChoice],
    params: &mut Params,
) -> Result<SubmissionKind, EngineError> {
    // The exam names an occurrence by its ability and, when written, its event;
    // the runner cannot read the event, so it never accepts an unchecked one.
    if !pending["event"].is_null() {
        return Err(untranslated(format!(
            "pending event {} cannot be checked",
            pending["event"]
        )));
    }
    let ability = &pending["ability"];
    let source = ability["source"].as_str().unwrap_or_default();
    let line = ability["line"].as_u64();
    let card = ability["card"].as_str();
    let labels = turn.labels();
    let matching: Vec<&PendingChoice> = choices
        .iter()
        .filter(|choice| {
            labels.pending_source(choice).as_deref() == Some(source)
                && line.is_none_or(|line| choice.line == line)
                && card.is_none_or(|card| choice.card == card)
        })
        .collect();
    // Several occurrences of one ability differ only by their event; the runner
    // cannot read that event, so it must not pick one for the exam.
    let choice = match matching.as_slice() {
        [] => return Err(adapter(format!("pending {ability} is not offered"))),
        [choice] => *choice,
        _ => {
            return Err(untranslated(format!(
                "pending {ability} matches {} occurrences",
                matching.len()
            )));
        }
    };
    let kind = turn.answer(question, Answer::Pending(choice.handle))?;
    turn.follow(params, kind)
}

#[expect(
    clippy::too_many_lines,
    reason = "One closed mapping from exam verbs to player commands."
)]
pub(super) fn act(
    turn: &mut Turn<'_>,
    decision: &Value,
    pending: &Value,
) -> Result<String, EngineError> {
    let verb = decision["do"].as_str().unwrap_or_default();
    let mut params = Params::of(decision);
    let refused = |attempted: &str| -> String {
        match attempted {
            "play" => "cannot-play",
            "activate" => "cannot-activate",
            "evolve" => "cannot-evolve",
            "attack" => "cannot-attack",
            _ => "pending-cancelled",
        }
        .into()
    };
    let question = turn.require()?;
    // Typed asks nothing when no card can be chosen; the exam still writes the empty answer.
    if verb == "resolve-choice"
        && decision["select"].as_array().is_some_and(Vec::is_empty)
        && !mid_resolution(&question)
    {
        return Ok(progress(turn.game));
    }
    let kind = match (verb, &question.shape) {
        ("play" | "activate", QuestionShape::Decision(DecisionShape::Quick { options, .. })) => {
            match quick_action(turn, verb, decision, &question, options, &mut params)? {
                Some(kind) => kind,
                None => return Ok(refused(verb)),
            }
        }
        ("pass", QuestionShape::Decision(DecisionShape::Quick { .. })) => {
            turn.answer(&question, Answer::Quick(None))?
        }
        (
            "play" | "activate" | "evolve" | "attack" | "end-phase",
            QuestionShape::ActionWindow(_),
        ) => match main_action(turn, verb, decision, &question, &mut params)? {
            Some(kind) => kind,
            None => return Ok(refused(verb)),
        },
        ("choose-pending", QuestionShape::Decision(DecisionShape::Pending(choices))) => {
            choose_pending(turn, pending, &question, choices, &mut params)?
        }
        (
            "order-replacements",
            QuestionShape::Decision(DecisionShape::DamageReplacements(choices)),
        ) => {
            let mut order = Vec::new();
            for wanted in decision["order"].as_array().into_iter().flatten() {
                let source = wanted["source"].as_str().unwrap_or_default();
                let line = wanted["line"].as_u64();
                let choice = choices
                    .iter()
                    .find(|choice| {
                        choice
                            .object
                            .as_ref()
                            .and_then(|handle| turn.label(handle))
                            .as_deref()
                            == Some(source)
                            && line.is_none_or(|line| choice.line == Some(line))
                    })
                    .ok_or_else(|| adapter(format!("replacement {wanted} is not offered")))?;
                order.push(choice.handle);
            }
            turn.answer(&question, Answer::Order(order))?
        }
        ("place-acted", QuestionShape::Decision(DecisionShape::WardEntry { .. })) => turn.answer(
            &question,
            Answer::WardEntry(decision["acted"].as_bool().unwrap_or_default()),
        )?,
        ("resolve-choice", QuestionShape::Decision(_) | QuestionShape::Evolution(_)) => {
            let kind = if let QuestionShape::Evolution(evolution) = &question.shape {
                turn.evolution(&question, evolution, &mut params)?
            } else {
                turn.parameter(&question, &mut params)?.ok_or_else(|| {
                    untranslated(format!(
                        "resolve-choice for {}",
                        shape_name(&question.shape)
                    ))
                })?
            };
            turn.follow(&mut params, kind)?
        }
        ("guard-act" | "end-discard", QuestionShape::Closing(closing)) => {
            let select = labels(&decision["select"]);
            let answer = match closing {
                ClosingQuestion::GuardAct(cards) => {
                    ClosingAnswer::GuardAct(turn.handles(cards.iter().map(|c| &c.handle), &select)?)
                }
                ClosingQuestion::Discard { cards, .. } => {
                    ClosingAnswer::Discard(turn.handles(cards.iter().map(|c| &c.handle), &select)?)
                }
            };
            turn.submit(Command::Closing {
                token: question.token,
                answer,
            })?
        }
        ("mulligan" | "choose-first" | "choose-start-amulet", QuestionShape::Opening(opening)) => {
            let answer = match opening {
                OpeningQuestion::FirstPlayer => {
                    OpeningAnswer::FirstPlayer(seat(&decision["first"])?)
                }
                OpeningQuestion::StartAmulet(cards) => {
                    let wanted = vec![decision["object"].as_str().unwrap_or_default().to_owned()];
                    let mut handles = turn.handles(cards.iter().map(|c| &c.handle), &wanted)?;
                    OpeningAnswer::StartAmulet(handles.remove(0))
                }
                OpeningQuestion::Mulligan(cards) => {
                    if decision["redo"] == true {
                        OpeningAnswer::Mulligan(turn.handles(
                            cards.iter().map(|c| &c.handle),
                            &labels(&decision["order"]),
                        )?)
                    } else {
                        OpeningAnswer::Keep
                    }
                }
            };
            turn.submit(Command::Opening {
                token: question.token,
                answer,
            })?
        }
        (_, shape) => {
            return Err(untranslated(format!("{verb} at {}", shape_name(shape))));
        }
    };
    Ok(match kind {
        SubmissionKind::Applied => progress(turn.game),
        SubmissionKind::Reverted => refused(verb),
        SubmissionKind::Cancelled => "pending-cancelled".into(),
        SubmissionKind::Rejected => {
            return Err(adapter(format!("{verb} answer rejected")));
        }
        SubmissionKind::EngineFault => return Err(fault(&*turn.game)),
    })
}

/// The exam writes one flat `select`; the runner may group it only when exactly one
/// assignment satisfies every group, so it never chooses among legal answers.
fn unique_grouping(
    groups: &[SelectionGroup],
    chosen: &[u64],
) -> Result<Vec<Vec<u64>>, EngineError> {
    let mut found = Vec::new();
    assign(
        groups,
        chosen,
        &mut vec![Vec::new(); groups.len()],
        &mut found,
    );
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(adapter("search choice fits no grouping")),
        _ => Err(untranslated("search choice has several groupings")),
    }
}

/// Collects complete group assignments of `rest`, stopping once two are known.
fn assign(
    groups: &[SelectionGroup],
    rest: &[u64],
    parts: &mut Vec<Vec<u64>>,
    found: &mut Vec<Vec<Vec<u64>>>,
) {
    if found.len() > 1 {
        return;
    }
    let Some((handle, tail)) = rest.split_first() else {
        let complete = groups.iter().zip(parts.iter()).all(|(group, part)| {
            u64::try_from(part.len()).is_ok_and(|len| len >= group.cardinality.minimum.0)
        });
        if complete {
            found.push(parts.clone());
        }
        return;
    };
    for (index, group) in groups.iter().enumerate() {
        let Some(part) = parts.get_mut(index) else {
            continue;
        };
        let room = group
            .cardinality
            .maximum
            .is_none_or(|max| u64::try_from(part.len()).is_ok_and(|len| len < max.0));
        if group.handles.contains(handle) && room {
            part.push(*handle);
            assign(groups, tail, parts, found);
            if let Some(undo) = parts.get_mut(index) {
                undo.pop();
            }
        }
    }
}

/// The exam outcome of an applied decision, read from where play now waits.
fn progress(game: &Game) -> String {
    if [Viewer::P1, Viewer::P2]
        .into_iter()
        .any(|seat| matches!(game.input(seat), Input::Finished { .. }))
    {
        "game-end".into()
    } else if [Viewer::P1, Viewer::P2]
        .into_iter()
        .any(|seat| matches!(game.input(seat), Input::Question(q) if mid_resolution(&q)))
    {
        "paused".into()
    } else {
        "resolved".into()
    }
}

/// Pass Quick windows the exam leaves implicit because Pass is the only legal answer.
pub(super) fn settle(game: &mut Game, decision: &Value, by: Viewer) -> Result<(), EngineError> {
    for _ in 0..MAX_QUESTIONS {
        let mut forced = None;
        for seat in [Viewer::P1, Viewer::P2] {
            match game.input(seat) {
                Input::EngineFault => return Err(fault(game)),
                Input::Question(question) => {
                    let explicit = seat == by && decision["at"] == "quick";
                    if !explicit
                        && let QuestionShape::Decision(DecisionShape::Quick { options, .. }) =
                            &question.shape
                        && options.is_empty()
                    {
                        forced = Some((seat, question.token));
                    }
                }
                Input::Waiting(_) | Input::Finished { .. } => {}
            }
        }
        let Some((seat, token)) = forced else {
            return Ok(());
        };
        let submission = game.submit_player(
            seat,
            Command::Answer {
                token,
                answer: Answer::Quick(None),
            },
        );
        if submission.kind != SubmissionKind::Applied {
            return Err(fault(game));
        }
    }
    Err(adapter("too many implicit Quick passes"))
}
