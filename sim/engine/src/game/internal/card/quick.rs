//! Rule-owned Quick questions; actions never accept caller-supplied sources or programs.
use super::super::{Answer, DecisionToken, DecisionView, InputDomain};
use super::{
    AbilityKind, CardPlay, Game, Generation, PlayerId, Result, int, invalid, printed_origin,
    seat_name, unsupported,
};
use crate::game::internal::observation::ObjectLink;
use crate::game::player::QuickChoice;
use crate::game::player::Viewer;
use crate::game::player::observation::zone;
use crate::ir::quick::{card_condition, card_permission, printed_activation};
use crate::ir::{Ability, Control, Located, Node, Program};
use crate::ir::{
    CardProgram, ChoiceDomain, Decision, DecisionId, DecisionKind, ExecutionId, PlayerRef,
    PlayerRelation, PlayerSet, Procedure, QuickWindow, Visibility, WellFormedProcedure,
};
use crate::ir::{Cardinality, Nat, TextQuantifier, ZoneKind};
use serde_json::{Value, json};

const QUICK_ZONES: [&str; 5] = ["field", "hand", "ex", "cemetery", "banish"];

/// A private action description addressed only by its question-local handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickOption {
    /// Valid only with this question's token.
    pub handle: u64,
    /// Printed card identity visible to its controller.
    pub card: String,
    /// Printed ability line for activation; absent for card play.
    pub line: Option<u64>,
    /// Current own zone, bound to the question's observation epoch.
    pub zone: String,
    /// One-based position in that zone, distinguishing identical copies without a fixed hidden ID.
    pub position: u64,
}

#[derive(Clone, Debug)]
struct Candidate {
    link: ObjectLink,
    holder: String,
    generation: Generation,
    ability: Option<super::super::admission::PlannedAbility>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct Question {
    pub(in crate::game::internal) view: DecisionView,
    procedure: WellFormedProcedure,
    continuation: Value,
    actions: Vec<Candidate>,
}

pub(super) fn check_spell_face(
    game: &Game,
    card_no: &str,
    card: &CardProgram,
    ordinal: usize,
    printed: &Value,
    permission: &Located<Ability>,
) -> Result<()> {
    if ordinal != 0
        || !matches!(
            printed["card_type"].as_str(),
            Some("スペル" | "スペル・トークン" | "スペル・アドバンス")
        )
        || !card.abilities.iter().any(|a| {
            a.node.kind == AbilityKind::Spell
                && printed_origin(&a.node.source.origin).is_ok_and(|(face, _)| {
                    game.printed_face_ordinal(card_no, face)
                        .is_ok_and(|face| face == ordinal)
                })
        })
        || card
            .abilities
            .iter()
            .filter(|a| card_permission(&a.node))
            .count()
            != 1
    {
        return Err(unsupported(
            "Quick requires one Spell on the same base face",
        ));
    }
    let condition =
        card_condition(&permission.node).ok_or_else(|| unsupported("Quick condition"))?;
    let program = Program {
        scope: permission.node.scope,
        steps: vec![Located {
            source: permission.source.clone(),
            node: Node::Control {
                control: Control::If {
                    condition: condition.clone(),
                    then_body: Program {
                        scope: permission.node.scope,
                        steps: vec![],
                    },
                    else_body: None,
                },
            },
        }],
    };
    super::super::admission::check_card(&program, &permission.source, &permission.node.source, None)
}

fn procedure(window: QuickWindow) -> Result<WellFormedProcedure> {
    let actor = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Active {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        }),
    };
    WellFormedProcedure::check(Procedure::Quick {
        window,
        decision: Decision {
            id: DecisionId(0),
            captures: Vec::new(),
            actor: actor.clone(),
            domain: ChoiceDomain::QuickActionOrPass {
                player: actor.clone(),
            },
            kind: DecisionKind::QuickWindow { window },
            visibility: Visibility::Only {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: actor,
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
            },
        },
    })
    .map_err(|failure| {
        invalid(format_args!(
            "internal Quick procedure invariant: {failure}"
        ))
    })
}

impl Game {
    pub(in crate::game) fn publicly_empty_quick(&self, actor: Viewer) -> bool {
        let Ok(seat) = seat_name(actor.player()) else {
            return false;
        };
        QUICK_ZONES
            .into_iter()
            .all(|zone| self.zone_count(seat, zone) == 0)
    }

    fn quick_window(&self) -> Option<QuickWindow> {
        if self.state.game["ended"] == true {
            return None;
        }
        match (
            self.state.flow["kind"].as_str(),
            self.state.flow["stage"].as_str(),
        ) {
            (Some("battle"), _) => Some(QuickWindow::Attack),
            (Some("end"), Some("quick")) => Some(QuickWindow::EndPhase),
            _ => None,
        }
    }

    pub(in crate::game::internal) fn internal_play_timing(
        &self,
        actor: PlayerId,
        quick: bool,
    ) -> bool {
        let Ok(seat) = seat_name(actor) else {
            return false;
        };
        (self.card_runtime.quick_start
            && quick
            && self.quick_window().is_some()
            && self.active() != seat)
            || (self.card_runtime.quick.is_none()
                && self.state.flow["kind"] == "main"
                && self.state.turn["phase"] == "main"
                && self.active() == seat)
    }

    pub(in crate::game::internal) fn internal_quick_spell(&self, holder: &str) -> Result<bool> {
        self.require_internal_abilities(holder, "Quick permission")?;
        for ability in self.internal_printed_abilities(holder)? {
            if let Some(condition) = card_condition(&ability.node) {
                let execution = self.provider_execution(holder, None)?;
                if execution.boolean(self, condition)?.0 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Ask the non-turn player after all checks and pending abilities have completed.
    /// Even a window without a legal play waits for an explicit Pass.
    ///
    /// # Errors
    /// Rejects an unfinished play/check or an unsupported whole-card intervention atomically.
    pub fn internal_quick_choice(&mut self) -> Result<Option<DecisionView>> {
        if self.typed.is_some()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.has_pending()
            || self.card_runtime.question.is_some()
            || !self.prepared.is_empty()
        {
            return Err(invalid("Quick waits for the complete check timing"));
        }
        if let Some(question) = &self.card_runtime.quick {
            return Ok(Some(question.view.clone()));
        }
        let Some(window) = self.quick_window() else {
            return Ok(None);
        };
        let before = self.clone();
        let result = self.prepare_quick(window);
        if result.is_err() {
            *self = before;
        }
        result.map(Some)
    }

    fn prepare_quick(&mut self, window: QuickWindow) -> Result<DecisionView> {
        self.check_internal_resources()?;
        self.check_internal_fixture()?;
        let actor = super::player_id(super::super::super::other(self.active()))?;
        let mut actions = Vec::new();
        let mut options = Vec::new();
        for holder in self.state.objects.keys() {
            let object = self.object(holder)?.clone();
            if object.controller != seat_name(actor)?
                || int(&object.state["face"]) != 0
                || object.state["silenced"] == true
                || !QUICK_ZONES.contains(&object.zone.as_str())
            {
                continue;
            }
            let abilities = self.internal_printed_abilities(holder)?;
            for located in abilities {
                let ability = &located.node;
                let spell = ability.kind == AbilityKind::Spell;
                let eligible = if spell {
                    object.owner == seat_name(actor)?
                        && matches!(
                            self.object_type(holder)?,
                            "スペル" | "スペル・トークン" | "スペル・アドバンス"
                        )
                        && (matches!(object.zone.as_str(), "hand" | "ex")
                            || self.internal_card_banish_play_allowed(holder, actor)?)
                        && self.internal_quick_spell(holder)?
                        && !self.restricted(holder, "play")?
                        && !self.internal_card_play_forbidden(holder, actor)?
                } else {
                    printed_activation(ability)
                        && matches!(
                            (object.zone.as_str(), ability.active_zones.as_slice()),
                            ("field", [ZoneKind::Field])
                                | ("hand", [ZoneKind::Hand])
                                | ("cemetery", [ZoneKind::Cemetery])
                        )
                        && super::quick_activation_type(
                            self.object_type(holder)?,
                            &ability.active_zones,
                        )
                };
                if !eligible || !self.internal_quick_preflight(holder, ability, actor, spell)? {
                    continue;
                }
                let handle = u64::try_from(actions.len()).map_err(invalid)?;
                options.push(QuickOption {
                    handle,
                    card: object.card.clone(),
                    line: if spell {
                        None
                    } else {
                        Some(printed_origin(&ability.source.origin)?.1.0)
                    },
                    zone: object.zone.clone(),
                    position: u64::try_from(
                        self.zone_ids(seat_name(actor)?, &object.zone)
                            .iter()
                            .position(|id| id == holder)
                            .ok_or_else(|| invalid("Quick source has no zone position"))?,
                    )
                    .map_err(invalid)?
                    .checked_add(1)
                    .ok_or_else(|| invalid("Quick position exhausted"))?,
                });
                actions.push(Candidate {
                    link: ObjectLink::capture(self, holder, object.generation),
                    holder: holder.clone(),
                    generation: Generation(object.generation),
                    ability: if spell { None } else { Some(located) },
                });
            }
        }
        let procedure = procedure(window)?;
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let view = DecisionView {
            token: DecisionToken {
                execution: ExecutionId(self.state.ir_epoch),
                decision: DecisionId(0),
                epoch: self.state.ir_epoch,
            },
            actor,
            domain: InputDomain::Quick { window, options },
        };
        self.card_runtime.quick = Some(Question {
            view: view.clone(),
            procedure,
            continuation: self.state.flow.clone(),
            actions,
        });
        Ok(view)
    }

    pub(in crate::game::internal) fn refresh_internal_quick(&mut self) -> Result<()> {
        if let Some(question) = &mut self.card_runtime.quick {
            self.state.ir_epoch = self
                .state
                .ir_epoch
                .checked_add(1)
                .ok_or_else(|| invalid("IR epoch exhausted"))?;
            question.view.token.epoch = self.state.ir_epoch;
        }
        Ok(())
    }

    pub(in crate::game::internal) fn answer_internal_quick(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: &Answer,
        validated: &mut bool,
    ) -> Result<CardPlay> {
        let question = self
            .card_runtime
            .quick
            .as_ref()
            .ok_or_else(|| invalid("no Quick question"))?;
        let Procedure::Quick { window, .. } = question.procedure.procedure() else {
            return Err(invalid("invalid Quick procedure"));
        };
        if question.view.actor != actor
            || question.view.token != token
            || question.continuation != self.state.flow
            || self.quick_window() != Some(*window)
        {
            return Err(invalid("wrong actor, procedure or stale Quick token"));
        }
        let Answer::Quick(handle) = answer else {
            return Err(invalid("Quick answer type"));
        };
        let action = handle
            .map(|handle| {
                question
                    .actions
                    .get(usize::try_from(handle).map_err(invalid)?)
                    .cloned()
                    .ok_or_else(|| invalid("foreign Quick handle"))
            })
            .transpose()?;
        if let Some(action) = &action
            && self.object(&action.holder)?.generation != action.generation.0
        {
            return Err(invalid("stale Quick source"));
        }
        let seat = seat_name(actor)?;
        *validated = true;
        let before = self.clone();
        self.card_runtime.quick = None;
        self.card_runtime.quick_start = true;
        let result = if let Some(action) = action {
            if let Some(ability) = action.ability {
                self.activate_loaded_card(
                    &action.holder,
                    &super::ActivationKey::Definition(ability.node.id),
                    actor,
                    &before.published_knowledge(),
                    &mut false,
                )
            } else {
                self.run_card_declaration(
                    &action.holder,
                    actor,
                    &before.published_knowledge(),
                    &mut false,
                )
            }
        } else {
            // Each Pass is a separate match input, so its events need their own causal node.
            let node = self.rule_node("internal-rule-");
            self.state.next_decision = self.state.next_decision.saturating_add(1);
            self.apply_rule_decision(&json!({"do":"pass","by":seat,"at":"quick"}), &node)
                .map(CardPlay::Continued)
        };
        self.card_runtime.quick_start = false;
        self.finish_card_attempt(before, result)
    }
}

pub(in crate::game::internal) fn packet(window: QuickWindow, options: &[QuickOption]) -> Value {
    json!({"kind":"quick","window":format!("{window:?}"),"pass":true,"options":options.iter().map(|o| json!({"handle":o.handle,"card":o.card,"line":o.line,"zone":o.zone,"position":o.position})).collect::<Vec<_>>()})
}

impl Question {
    pub(in crate::game::internal) fn player_options(
        &self,
        game: &Game,
        actor: Viewer,
        options: &[QuickOption],
    ) -> Result<Vec<QuickChoice>> {
        if options.len() != self.actions.len() {
            return Err(invalid("inconsistent Quick coordinates"));
        }
        options
            .iter()
            .zip(&self.actions)
            .map(|(option, candidate)| {
                Ok(QuickChoice {
                    handle: option.handle,
                    card: option.card.clone(),
                    line: option.line,
                    zone: zone(&option.zone)?,
                    position: option.position,
                    object: candidate.link.observed(game, actor),
                    source: candidate
                        .ability
                        .as_ref()
                        .map(|definition| {
                            game.activation_text_source(
                                &candidate.holder,
                                &definition.clone().into(),
                                actor,
                            )
                        })
                        .transpose()?
                        .flatten(),
                })
            })
            .collect()
    }
}
