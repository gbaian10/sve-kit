//! Normal evolution stages borrow the native material validators without trial payment.

use super::{intrinsic, rule_code, usable};
use crate::game::int;
use crate::game::internal::{Context, Execution, Game, InputDomain, Result, Wait};
use crate::game::player::observation::{CardObservation, seat};
use crate::game::player::{
    EvolutionAnswer, EvolutionForm, EvolutionFormId, EvolutionPayment, EvolutionQuestion,
    QuestionToken, Viewer,
};
use crate::ir::{
    Ability, AbilityId, Action, CostSpec, Generation, ObjectRef, PlaySpec, Resource,
    ResourceOperation,
};
use crate::{EngineFailure, invalid};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
struct Form {
    holder: String,
    generation: u64,
    card: CardObservation,
    face: usize,
    name: String,
}

#[derive(Clone, Debug)]
enum Stage {
    Form(Vec<Form>),
    Materials,
    Discard,
    Payment(EvolutionPayment),
}

#[derive(Clone, Debug)]
pub(in crate::game) struct Declaration {
    actor: Viewer,
    source: Context,
    definition: AbilityId,
    code: Value,
    play: PlaySpec,
    selection: Execution,
    form: Option<Form>,
    parameter: usize,
    materials: Vec<ObjectRef>,
    discard: Vec<ObjectRef>,
    hand_done: bool,
    stage: Stage,
}

#[derive(Clone, Debug)]
pub(in crate::game) struct EvolutionOption {
    pub(in crate::game) ordinal: u64,
    pub(in crate::game) line: u64,
}

fn illegal(message: &str) -> EngineFailure {
    EngineFailure::IllegalPlay(message.into())
}

fn payment_valid(domain: &EvolutionPayment, pp: i64, ep: bool, super_evolution: bool) -> bool {
    pp >= 0
        && pp <= domain.available_pp
        && pp.checked_add(i64::from(ep)) == Some(domain.total_pp)
        && (!ep || domain.ep_allowed)
        && (!super_evolution || domain.super_allowed)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Normal evolution keeps its native rule authority."
)]
impl Game {
    fn evolution_quote(&self, source: &str, code: &Value) -> Result<EvolutionPayment> {
        let actor = &self.object(source)?.controller;
        let probe = self.clone();
        // Current evolution cost readers depend on the source, never the selected form or face.
        let decision = json!({"do":"evolve","source":source,"by":actor,"at":"main"});
        let mut frame = probe.start_frame(source, probe.reference(source, code), &decision)?;
        probe.freeze(code, "play-start", &mut frame)?;
        let total_pp = probe.evolution_pp(code, &frame)?;
        let player = self.player(actor)?;
        Ok(EvolutionPayment {
            total_pp,
            available_pp: int(&player.pp["current"]),
            ep_allowed: player.ep > 0 && total_pp > 0,
            super_allowed: player.sep > 0 && self.can_super_evolve(actor),
        })
    }

    fn evolution_forms(&self, source: &str, actor: Viewer, code: &Value) -> Result<Vec<Form>> {
        let mut forms = Vec::new();
        for holder in self.zone_ids(seat(actor), "evolve_deck") {
            if !self.player_knows_object(actor, &holder) {
                continue;
            }
            let object = self.object(&holder)?;
            let card = self.main_object(actor, &holder)?;
            let count = self
                .catalog
                .cards
                .get(&object.card)
                .map_or(0, |definition| definition.faces.len());
            for face in 0..count {
                if self.evolution_pair(source, &holder, face, code)? {
                    forms.push(Form {
                        holder: holder.clone(),
                        generation: object.generation,
                        card: card.clone(),
                        face,
                        name: self.catalog.rules_label(&object.card, face)?,
                    });
                }
            }
        }
        forms.sort_by(|left, right| {
            (
                &left.card.properties.card,
                left.face,
                &left.name,
                &left.card.handle,
            )
                .cmp(&(
                    &right.card.properties.card,
                    right.face,
                    &right.name,
                    &right.card.handle,
                ))
        });
        Ok(forms)
    }

    fn main_evolution_entries(&self, holder: &str, actor: Viewer) -> Result<Vec<Ability>> {
        let object = self.object(holder)?;
        if object.controller != seat(actor)
            || object.zone != "field"
            || object.state["evolved_with"].is_string()
            || object.state["face"] != 0_i64
            || !self.card_runtime.is_loaded(&object.card)
        {
            return Ok(vec![]);
        }
        let mut entries = Vec::new();
        for entry in self.internal_printed_abilities(holder)? {
            if !intrinsic(&entry.node) || !usable(self, holder, &entry.node)? {
                continue;
            }
            let code = rule_code(&entry.node)?;
            let quote = self.evolution_quote(holder, &code)?;
            if quote
                .available_pp
                .saturating_add(i64::from(quote.ep_allowed))
                < quote.total_pp
                || self.evolution_forms(holder, actor, &code)?.is_empty()
            {
                continue;
            }
            entries.push(entry.node.clone());
        }
        entries.sort_by_key(|entry| {
            super::super::printed_origin(&entry.source.origin).map_or(u64::MAX, |(_, line)| line.0)
        });
        Ok(entries)
    }

    pub(in crate::game) fn main_evolution_options(
        &self,
        holder: &str,
        actor: Viewer,
    ) -> Result<Vec<EvolutionOption>> {
        self.main_evolution_entries(holder, actor)?
            .iter()
            .enumerate()
            .map(|(ordinal, entry)| {
                Ok(EvolutionOption {
                    ordinal: u64::try_from(ordinal).map_err(invalid)?,
                    line: super::super::printed_origin(&entry.source.origin)?.1.0,
                })
            })
            .collect()
    }

    pub(in crate::game) fn begin_main_evolution(
        &mut self,
        holder: &str,
        ordinal: u64,
        actor: Viewer,
    ) -> Result<bool> {
        let entry = self
            .main_evolution_entries(holder, actor)?
            .into_iter()
            .nth(usize::try_from(ordinal).map_err(invalid)?)
            .ok_or_else(|| invalid("retired evolution option"))?;
        let code = rule_code(&entry)?;
        let forms = self.evolution_forms(holder, actor, &code)?;
        let object = self.object(holder)?;
        let source = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor.player(),
        };
        let mut probe = self.clone();
        let mut selection = probe.new_card_execution(source.clone())?;
        let origin = probe.internal_card_source(holder, entry.id, actor.player())?;
        selection.bind_source(&probe, origin)?;
        let mut play = entry
            .play
            .ok_or_else(|| invalid("missing evolution play"))?;
        // Admission already permits one face parameter and at most one material parameter.
        if play.parameters.is_empty() || play.parameters.len() > 2 {
            return Err(super::super::unsupported("evolution material parameters"));
        }
        play.parameters.remove(0);
        let CostSpec::Ordered { steps } = &mut play.costs else {
            return Err(invalid("evolution costs"));
        };
        if steps.is_empty() {
            return Err(invalid("evolution reveal cost"));
        }
        steps.remove(0);
        steps.retain(|step| {
            !matches!(
                step.instruction.action,
                Action::Resource {
                    resource: Resource::PpCurrent,
                    operation: ResourceOperation::Pay,
                    ..
                }
            )
        });
        self.player_evolution = Some(Box::new(Declaration {
            actor,
            source,
            definition: entry.id,
            code,
            play,
            selection,
            form: None,
            parameter: 0,
            materials: vec![],
            discard: vec![],
            hand_done: false,
            stage: Stage::Form(forms),
        }));
        Ok(true)
    }

    pub(in crate::game) fn evolution_actor(&self) -> Option<Viewer> {
        self.player_evolution
            .as_ref()
            .map(|declaration| declaration.actor)
    }

    pub(in crate::game) fn evolution_view(
        &self,
        token: QuestionToken,
    ) -> Result<EvolutionQuestion> {
        let declaration = self
            .player_evolution
            .as_ref()
            .ok_or_else(|| invalid("missing evolution declaration"))?;
        match &declaration.stage {
            Stage::Form(forms) => Ok(EvolutionQuestion::Form(
                forms
                    .iter()
                    .enumerate()
                    .map(|(ordinal, form)| {
                        Ok(EvolutionForm {
                            id: EvolutionFormId::new(token, ordinal),
                            card: form.card.clone(),
                            face: u64::try_from(form.face).map_err(invalid)?,
                            name: form.name.clone(),
                        })
                    })
                    .collect::<Result<_>>()?,
            )),
            Stage::Materials | Stage::Discard => {
                let domain =
                    self.player_evolution_selection(declaration.actor, &declaration.selection)?;
                Ok(if matches!(declaration.stage, Stage::Materials) {
                    EvolutionQuestion::Materials(domain)
                } else {
                    EvolutionQuestion::Discard(domain)
                })
            }
            Stage::Payment(payment) => Ok(EvolutionQuestion::Payment(*payment)),
        }
    }

    pub(in crate::game) fn answer_main_evolution(
        &mut self,
        token: QuestionToken,
        answer: EvolutionAnswer,
        validated: &mut bool,
    ) -> Result<bool> {
        let mut declaration = self
            .player_evolution
            .take()
            .ok_or_else(|| invalid("missing evolution declaration"))?;
        match (&declaration.stage, answer) {
            (Stage::Form(forms), EvolutionAnswer::Form(id)) => {
                let form = forms
                    .get(id.ordinal(token)?)
                    .ok_or_else(|| invalid("foreign evolution form"))?
                    .clone();
                *validated = true;
                declaration.form = Some(form);
            }
            (Stage::Materials, EvolutionAnswer::Materials(handles)) => {
                declaration.material_answer(self, &handles, false)?;
                *validated = true;
                declaration.parameter = declaration
                    .parameter
                    .checked_add(1)
                    .ok_or_else(|| invalid("evolution material parameter overflow"))?;
            }
            (Stage::Discard, EvolutionAnswer::Discard(handles)) => {
                declaration.material_answer(self, &handles, true)?;
                *validated = true;
                declaration.hand_done = true;
            }
            (
                Stage::Payment(payment),
                EvolutionAnswer::Payment {
                    pp,
                    ep,
                    super_evolution,
                },
            ) => {
                if !payment_valid(payment, pp, ep, super_evolution) {
                    return Err(invalid("evolution payment answer"));
                }
                *validated = true;
                declaration.validate_source(self)?;
                declaration.validate_references(self)?;
                let form = declaration
                    .form
                    .as_ref()
                    .ok_or_else(|| invalid("missing selected evolution form"))?;
                let materials = declaration.physical(self, &declaration.materials)?;
                let discard = declaration.physical(self, &declaration.discard)?;
                self.node = format!("player-evolution-{}", self.state.next_decision);
                self.state.next_decision = self.state.next_decision.saturating_add(1);
                self.emitted.clear();
                return self.internal_evolve_action(&json!({"do":"evolve","source":declaration.source.source,"definition":declaration.definition.0,"evolve_card":form.holder,"face":form.face,"by":seat(declaration.actor),"at":"main","pay":{"pp":pp,"ep":i64::from(ep),"sep":i64::from(super_evolution)},"costs":{"materials":materials,"discard":discard}}));
            }
            _ => return Err(invalid("evolution answer shape")),
        }
        declaration.validate_source(self)?;
        declaration.next_stage(self)?;
        self.player_evolution = Some(declaration);
        Ok(true)
    }
}

impl Declaration {
    fn validate_source(&self, game: &Game) -> Result<()> {
        let source = game.object(&self.source.source)?;
        if source.generation != self.source.generation.0
            || source.controller != seat(self.actor)
            || source.zone != "field"
            || game.state.flow["kind"] != "main"
            || game.state.turn["phase"] != "main"
            || game.active() != seat(self.actor)
        {
            return Err(illegal("evolution source changed"));
        }
        Ok(())
    }

    fn physical(&self, game: &Game, references: &[ObjectRef]) -> Result<Vec<String>> {
        references
            .iter()
            .map(|reference| {
                let id = self.selection.object_id(*reference)?;
                if game.object(id)?.generation != reference.generation.0 {
                    return Err(illegal("evolution material generation changed"));
                }
                Ok(id.into())
            })
            .collect()
    }

    fn validate_references(&self, game: &Game) -> Result<()> {
        let form = self
            .form
            .as_ref()
            .ok_or_else(|| invalid("missing selected evolution form"))?;
        if game.object(&form.holder)?.generation != form.generation {
            return Err(illegal("evolution form generation changed"));
        }
        self.physical(game, &self.materials)?;
        self.physical(game, &self.discard)?;
        Ok(())
    }

    fn material_answer(&mut self, game: &Game, handles: &[u64], discard: bool) -> Result<()> {
        let Some((view, wait)) = self.selection.waiting.clone() else {
            return Err(invalid("missing evolution selection"));
        };
        let InputDomain::CostMaterials(domain) = view.domain else {
            return Err(invalid("evolution selection domain"));
        };
        if handles
            .iter()
            .any(|handle| !domain.members.iter().any(|member| member.handle == *handle))
        {
            return Err(invalid("foreign evolution selection handle"));
        }
        match (wait, discard) {
            (Wait::MaterialParameter(wait), false) => {
                let members = wait.validate(handles)?;
                self.materials.clone_from(&members);
                wait.bind(&mut self.selection, members);
            }
            (Wait::CostDiscard(wait), true) => {
                let members = wait.validate(game, &self.selection, handles)?;
                self.discard.clone_from(&members);
                self.selection
                    .cost_materials
                    .insert(wait.receipt(), members);
            }
            _ => return Err(invalid("evolution selection authority")),
        }
        self.selection.waiting = None;
        Ok(())
    }

    fn next_stage(&mut self, game: &Game) -> Result<()> {
        self.validate_references(game)?;
        let mut probe = game.clone();
        probe.card_runtime.active_source = self.selection.ability;
        if let Some(parameter) = self.play.parameters.get(self.parameter) {
            probe.internal_material_parameter(
                &mut self.selection,
                parameter,
                super::super::material::posture_parameter(&self.play.costs, parameter),
            )?;
            let source = self.selection.reference(game, &self.source.source)?;
            if let Some((view, Wait::MaterialParameter(wait))) = self.selection.waiting.as_mut() {
                let InputDomain::CostMaterials(domain) = &mut view.domain else {
                    return Err(invalid("evolution material domain"));
                };
                let forbidden = wait.handles_for(source)?;
                domain
                    .members
                    .retain(|member| !forbidden.contains(&member.handle));
                if u64::try_from(domain.members.len()).map_err(invalid)?
                    < domain.cardinality.minimum.0
                {
                    return Err(illegal("insufficient evolution materials"));
                }
            }
            self.stage = Stage::Materials;
            return Ok(());
        }
        if !self.hand_done
            && probe.internal_hand_cost_materials(&mut self.selection, &self.play.costs)?
        {
            self.stage = Stage::Discard;
            return Ok(());
        }
        self.stage = Stage::Payment(game.evolution_quote(&self.source.source, &self.code)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
