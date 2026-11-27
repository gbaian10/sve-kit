//! The menu and live evolution share the same form, material and payment stages.

use super::super::declarations::probe::{MAX_ANSWERS, cost_selections};
use super::{cost, destination, intrinsic, non_pp_play, usable};
use crate::game::int;
use crate::game::internal::{Context, DeclarationMode, Execution, Game, InputDomain, Result, Wait};
use crate::game::player::observation::{CardObservation, EffectSource, seat};
use crate::game::player::{
    EvolutionAnswer, EvolutionForm, EvolutionFormId, EvolutionPayment, EvolutionQuestion,
    ObjectHandle, QuestionToken, Viewer,
};
use crate::ir::evolution_destination::Destination;
use crate::ir::{Ability, AbilityId, CostSpec, Decision, ObjectRef, PlaySpec};
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
    base_pp: i64,
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
    pub(in crate::game) source: Option<EffectSource>,
    pub(in crate::game) ordinal: u64,
    pub(in crate::game) line: u64,
}

fn illegal(message: &str) -> EngineFailure {
    EngineFailure::IllegalPlay(message.into())
}

impl Game {
    fn evolution_quote(&self, source: &str, base_pp: i64) -> Result<EvolutionPayment> {
        let actor = &self.object(source)?.controller;
        let probe = self.clone();
        // Current evolution cost readers depend on the source, never the selected form or face.
        let decision = json!({"do":"evolve","source":source,"by":actor,"at":"main"});
        let frame = probe.start_frame(source, probe.reference(source, &Value::Null), &decision)?;
        let total_pp = probe.evolution_pp(base_pp, &frame)?;
        let player = self.player(actor)?;
        Ok(EvolutionPayment {
            total_pp,
            available_pp: int(&player.pp["current"]),
            ep_allowed: player.ep > 0 && total_pp > 0,
            super_allowed: player.sep > 0 && self.can_super_evolve(actor),
        })
    }

    fn evolution_forms(
        &self,
        source: &str,
        actor: Viewer,
        destination: &Destination,
    ) -> Result<Vec<Form>> {
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
                if self.evolution_pair(source, &holder, face, destination)? {
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
            entries.push(entry.node.clone());
        }
        entries.sort_by_key(|entry| {
            super::super::printed_origin(&entry.source.origin).map_or(u64::MAX, |(_, line)| line.0)
        });
        Ok(entries)
    }

    fn prepare_evolution_materials(
        &self,
        selection: &mut Execution,
        costs: &CostSpec,
        parameter: &Decision,
    ) -> Result<()> {
        self.internal_material_parameter(
            selection,
            parameter,
            super::super::material::posture_parameter(costs, parameter),
        )?;
        let source = selection.reference(self, &selection.context.source)?;
        let Some((view, Wait::MaterialParameter(wait))) = selection.waiting.as_mut() else {
            return Err(invalid("missing evolution material question"));
        };
        let InputDomain::CostMaterials(domain) = &mut view.domain else {
            return Err(invalid("evolution material domain"));
        };
        let forbidden = wait.handles_for(source)?;
        domain
            .members
            .retain(|member| !forbidden.contains(&member.handle));
        if u64::try_from(domain.members.len()).map_err(invalid)? < domain.cardinality.minimum.0 {
            return Err(illegal("insufficient evolution materials"));
        }
        Ok(())
    }

    pub(in crate::game) fn main_evolution_options(
        &self,
        holder: &str,
        actor: Viewer,
    ) -> Result<Vec<EvolutionOption>> {
        let mut options = Vec::new();
        for (ordinal, entry) in self
            .main_evolution_entries(holder, actor)?
            .iter()
            .enumerate()
        {
            let mut declaration = self.prepare_main_evolution(holder, actor, entry)?;
            declaration.selection.declaration_mode = DeclarationMode::Probe;
            let mut budget = MAX_ANSWERS;
            if declaration.has_payment(self, &mut budget)? {
                options.push(EvolutionOption {
                    source: declaration
                        .selection
                        .source_information
                        .as_ref()
                        .ok_or_else(|| invalid("missing evolution source"))?
                        .origin()
                        .observed(self, actor),
                    ordinal: u64::try_from(ordinal).map_err(invalid)?,
                    line: super::super::printed_origin(&entry.source.origin)?.1.0,
                });
            }
        }
        Ok(options)
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
        let declaration = self.prepare_main_evolution(holder, actor, &entry)?;
        if matches!(&declaration.stage, Stage::Form(forms) if forms.is_empty()) {
            return Ok(false);
        }
        self.player_evolution = Some(Box::new(declaration));
        Ok(true)
    }

    fn prepare_main_evolution(
        &self,
        holder: &str,
        actor: Viewer,
        entry: &Ability,
    ) -> Result<Declaration> {
        let destination = destination(entry)?;
        let base_pp = cost(entry)?;
        let forms = self.evolution_forms(holder, actor, &destination)?;
        let mut probe = self.clone();
        let selection = probe.evolution_selection(holder, entry.id, actor.player())?;
        let source = selection.context.clone();
        let play = non_pp_play(entry)?;
        Ok(Declaration {
            actor,
            source,
            definition: entry.id,
            base_pp,
            play,
            selection,
            form: None,
            parameter: 0,
            materials: vec![],
            discard: vec![],
            hand_done: false,
            stage: Stage::Form(forms),
        })
    }

    pub(in crate::game) fn evolution_actor(&self) -> Option<Viewer> {
        self.player_evolution
            .as_ref()
            .map(|declaration| declaration.actor)
    }

    pub(in crate::game) fn evolution_subject(&self, viewer: Viewer) -> Result<ObjectHandle> {
        let declaration = self
            .player_evolution
            .as_ref()
            .ok_or_else(|| invalid("missing evolution declaration"))?;
        declaration.validate_source(self)?;
        // Each viewer owns a different identity period for the same public base.
        self.observed_object_handle(viewer, &declaration.source.source)
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
                declaration.choose_materials(self, &handles, false)?;
                *validated = true;
            }
            (Stage::Discard, EvolutionAnswer::Discard(handles)) => {
                declaration.choose_materials(self, &handles, true)?;
                *validated = true;
            }
            (
                Stage::Payment(payment),
                EvolutionAnswer::Payment {
                    pp,
                    ep,
                    super_evolution,
                },
            ) => {
                if !payment.is_valid(pp, ep, super_evolution) {
                    return Err(invalid("evolution payment answer"));
                }
                *validated = true;
                let decision = declaration.payment_decision(self, pp, ep, super_evolution)?;
                self.node = self.rule_node("player-evolution-");
                self.state.next_decision = self.state.next_decision.saturating_add(1);
                self.emitted.clear();
                return self.internal_evolve_action(&decision);
            }
            _ => return Err(invalid("evolution answer shape")),
        }
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

    fn payment_decision(
        &self,
        game: &Game,
        pp: i64,
        ep: bool,
        super_evolution: bool,
    ) -> Result<Value> {
        self.validate_source(game)?;
        self.validate_references(game)?;
        let form = self
            .form
            .as_ref()
            .ok_or_else(|| invalid("missing selected evolution form"))?;
        let materials = self.physical(game, &self.materials)?;
        let discard = self.physical(game, &self.discard)?;
        Ok(json!({
            "do":"evolve", "source":self.source.source, "definition":self.definition.0,
            "evolve_card":form.holder, "face":form.face, "by":seat(self.actor), "at":"main",
            "pay":{"pp":pp,"ep":i64::from(ep),"sep":i64::from(super_evolution)},
            "costs":{"materials":materials,"discard":discard}
        }))
    }

    fn has_payment(&self, game: &Game, budget: &mut u32) -> Result<bool> {
        match &self.stage {
            Stage::Form(forms) => {
                for form in forms {
                    if *budget == 0 {
                        break;
                    }
                    *budget = budget.saturating_sub(1);
                    let mut branch = self.clone();
                    branch.form = Some(form.clone());
                    if branch.probe_next_stage(game, budget)? {
                        return Ok(true);
                    }
                }
            }
            Stage::Materials | Stage::Discard => {
                let discard = matches!(self.stage, Stage::Discard);
                let Some((view, wait)) = &self.selection.waiting else {
                    return Err(invalid("missing evolution selection"));
                };
                let InputDomain::CostMaterials(domain) = &view.domain else {
                    return Err(invalid("evolution selection domain"));
                };
                for handles in cost_selections(domain, wait) {
                    if *budget == 0 {
                        break;
                    }
                    *budget = budget.saturating_sub(1);
                    let mut branch = self.clone();
                    match branch.choose_materials(game, &handles, discard) {
                        Ok(()) if branch.probe_next_stage(game, budget)? => return Ok(true),
                        Ok(()) | Err(EngineFailure::Invalid(_) | EngineFailure::IllegalPlay(_)) => {
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            Stage::Payment(payment) => {
                for ep in [false, true] {
                    let pp = payment.total_pp.saturating_sub(i64::from(ep));
                    if *budget == 0 {
                        break;
                    }
                    if payment.is_valid(pp, ep, false) {
                        *budget = budget.saturating_sub(1);
                        let decision = self.payment_decision(game, pp, ep, false)?;
                        match game
                            .declaration_probe()
                            .run_internal_evolution(&decision, true)
                        {
                            Ok(true) => return Ok(true),
                            Ok(false) | Err(EngineFailure::IllegalPlay(_)) => {}
                            Err(error) => return Err(error),
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    fn probe_next_stage(&mut self, game: &Game, budget: &mut u32) -> Result<bool> {
        match self.next_stage(game) {
            Ok(()) => self.has_payment(game, budget),
            Err(EngineFailure::IllegalPlay(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn choose_materials(&mut self, game: &Game, handles: &[u64], discard: bool) -> Result<()> {
        self.material_answer(game, handles, discard)?;
        if discard {
            self.hand_done = true;
        } else {
            self.parameter = self
                .parameter
                .checked_add(1)
                .ok_or_else(|| invalid("evolution material parameter overflow"))?;
        }
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
                wait.record(&mut self.selection, members);
            }
            _ => return Err(invalid("evolution selection authority")),
        }
        self.selection.waiting = None;
        Ok(())
    }

    fn next_stage(&mut self, game: &Game) -> Result<()> {
        self.validate_source(game)?;
        self.validate_references(game)?;
        let mut probe = game.clone();
        probe.card_runtime.active_source = self.selection.ability;
        if let Some(parameter) = self.play.parameters.get(self.parameter) {
            probe.prepare_evolution_materials(&mut self.selection, &self.play.costs, parameter)?;
            self.stage = Stage::Materials;
            return Ok(());
        }
        if !self.hand_done
            && probe.internal_hand_cost_materials(&mut self.selection, &self.play.costs)?
        {
            self.stage = Stage::Discard;
            return Ok(());
        }
        self.stage = Stage::Payment(game.evolution_quote(&self.source.source, self.base_pp)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
