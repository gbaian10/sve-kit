//! Rule-owned Quick questions; actions never accept caller-supplied sources or programs.
use super::super::{Answer, DecisionToken, DecisionView, InputDomain};
use super::{
    AbilityKind, CardPlay, Game, Generation, PlayerId, Result, int, invalid, printed_origin,
    seat_name, unsupported,
};
use crate::ir::quick::{card_permission, is_printed_quick};
use crate::ir::{AbilityId, Cardinality, Nat, TextQuantifier};
use crate::ir::{
    CardProgram, ChoiceDomain, Decision, DecisionId, DecisionKind, ExecutionId, PlayerRef,
    PlayerRelation, PlayerSet, Procedure, QuickWindow, Visibility, WellFormedProcedure,
};
use serde_json::{Value, json};

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
    holder: String,
    generation: Generation,
    ability: Option<AbilityId>,
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
    Ok(())
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
    .map_err(invalid)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Quick shares the formal play authority."
)]
impl Game {
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
        let object = self.object(holder)?;
        Ok(self
            .card_runtime
            .cards
            .get(&object.card)
            .is_some_and(|card| {
                card.card()
                    .abilities
                    .iter()
                    .any(|a| card_permission(&a.node))
            }))
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
        self.internal_prototype_abilities()?;
        let actor = super::player_id(super::super::super::other(self.active()))?;
        let mut actions = Vec::new();
        let mut options = Vec::new();
        for holder in self.state.objects.keys() {
            let object = self.object(holder)?.clone();
            if object.controller != seat_name(actor)?
                || int(&object.state["face"]) != 0
                || object.state["evolved_with"].is_string()
                || object.state["silenced"] == true
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
                        && matches!(object.zone.as_str(), "hand" | "ex")
                        && self.internal_quick_spell(holder)?
                        && !self.restricted(holder, "play")?
                } else {
                    ability.kind == AbilityKind::Activated
                        && is_printed_quick(ability)
                        && object.zone == "field"
                        && self.object_type(holder)? == "フォロワー"
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
                    holder: holder.clone(),
                    generation: Generation(object.generation),
                    ability: if spell { None } else { Some(ability.id) },
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
        let before = self.clone();
        self.card_runtime.quick = None;
        self.card_runtime.quick_start = true;
        let result = if let Some(action) = action {
            if let Some(ability) = action.ability {
                self.activate_loaded_card(&action.holder, ability, actor)
            } else {
                self.play_internal_spell(&action.holder, actor)
            }
        } else {
            // Each Pass is a separate match input, so its events need their own causal node.
            let node = format!("internal-rule-{}", self.state.next_decision);
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
