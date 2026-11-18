//! Frame existence, public progress and private participation require separate witnesses.

use super::payment_observation::{PaymentBoundary, completed};
use super::{Execution, Game, PlayDeclaration, PlayPhase, QuestionStage};
use crate::game::internal::{
    Wait,
    fact_log::viewer,
    observation::{EffectOrigin, ObjectLink, SourceIdentity},
    source::SourceInformation,
};
use crate::game::player::Viewer;
use crate::game::player::observation::{
    ProcessingContext, ProcessingDeclaration, ProcessingPayment, ProcessingStage, ProcessingSubject,
};
use crate::ir::{ObjectRef, RuntimeDamageRecipient};
use alloc::collections::{BTreeMap, btree_map::Entry};

#[derive(Clone, Debug)]
struct SubjectEvidence {
    identity: SourceIdentity,
    link: ObjectLink,
}

impl SubjectEvidence {
    fn capture(game: &Game, reference: ObjectRef) -> Option<Self> {
        Some(Self {
            identity: SourceIdentity::capture(game, reference)?,
            link: ObjectLink::reference(game, reference.id.0, reference.generation.0)?,
        })
    }

    fn observed(&self, game: &Game, perspective: Viewer) -> Option<ProcessingSubject> {
        Some(ProcessingSubject::Card {
            card: self.identity.observed(perspective)?,
            object: self.link.observed(game, perspective),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct Disclosure {
    origin: Option<EffectOrigin>,
    subject: Option<SubjectEvidence>,
    stage: Option<ProcessingStage>,
    payments: Option<PaymentBoundary>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::game::internal) struct ProcessingDisclosure {
    viewers: BTreeMap<Viewer, Disclosure>,
    subjects: BTreeMap<(u64, u64), SubjectEvidence>,
    pub(in crate::game::internal) direct_payments: Vec<ProcessingPayment>,
    pub(in crate::game::internal) procedures: super::procedures::Procedures,
}

// execute_internal, answer_internal and card_play_result publish real waits; probes do not.
pub(in crate::game::internal) fn disclose_source(game: &Game, execution: &mut Execution) {
    if execution.play_phase.is_none() {
        return;
    }
    let Some((question, _)) = &execution.waiting else {
        return;
    };
    let Some(actor) = viewer(question.actor) else {
        return;
    };
    disclose_to(game, execution, [actor], None);
}

pub(in crate::game::internal) fn disclose_declared_source(game: &Game, execution: &mut Execution) {
    disclose_public(game, execution, ProcessingStage::Declare);
}

pub(in crate::game::internal) fn disclose_played_source(game: &Game, execution: &mut Execution) {
    disclose_public(game, execution, ProcessingStage::Resolve);
    // Freeze the published prefix; reading a projection must not change rule execution.
    let payments = PaymentBoundary::capture(execution);
    for disclosure in execution.processing_disclosure.viewers.values_mut() {
        if disclosure.stage.is_some() {
            disclosure.payments = Some(payments);
        }
    }
}

fn disclose_public(game: &Game, execution: &mut Execution, stage: ProcessingStage) {
    // Match fact_log::event_source's frozen public-holder gate, not current source visibility.
    // Field plays have no SourceInformation; both calls check while the card is in public Resolution.
    let public = execution
        .source_information
        .as_ref()
        .is_some_and(SourceInformation::publicly_identified)
        || (execution.field_entry
            && [Viewer::P1, Viewer::P2].into_iter().all(|perspective| {
                game.player_knows_object(perspective, &execution.context.source)
            }));
    if public {
        disclose_to(game, execution, [Viewer::P1, Viewer::P2], Some(stage));
    }
}

fn disclose_to(
    game: &Game,
    execution: &mut Execution,
    audience: impl IntoIterator<Item = Viewer>,
    stage: Option<ProcessingStage>,
) {
    let origin = execution
        .source_information
        .as_ref()
        .map(SourceInformation::origin);
    let subject = execution
        .ability
        .map(|source| source.holder)
        .or_else(|| execution.reference(game, &execution.context.source).ok())
        .and_then(|reference| SubjectEvidence::capture(game, reference));
    for perspective in audience {
        let origin = origin.filter(|origin| origin.observed(game, perspective).is_some());
        // A public holder alone does not disclose a secretly granted ability's existence.
        if origin.is_none() && !execution.field_entry {
            continue;
        }
        let subject = subject
            .as_ref()
            .filter(|subject| subject.identity.observed(perspective).is_some());
        if origin.is_none() && subject.is_none() {
            continue;
        }
        let disclosure = execution
            .processing_disclosure
            .viewers
            .entry(perspective)
            .or_default();
        if disclosure.origin.is_none() {
            disclosure.origin = origin.cloned();
        }
        if disclosure.subject.is_none() {
            disclosure.subject = subject.cloned();
        }
        if stage.is_some() {
            disclosure.stage = stage;
        }
    }
}

/// Capture accepted targets before payment can move them; observation must not rebind them later.
pub(in crate::game::internal) fn capture_declarations(game: &Game, execution: &mut Execution) {
    let mut references = Vec::new();
    for declaration in &execution.target_declarations {
        references.extend_from_slice(&declaration.objects);
    }
    for declaration in &execution.play_declarations {
        match declaration {
            PlayDeclaration::Recipients { members, .. } => {
                references.extend(members.iter().filter_map(|member| match member {
                    RuntimeDamageRecipient::Object { object } => Some(*object),
                    RuntimeDamageRecipient::Leader { .. } => None,
                }));
            }
            PlayDeclaration::Distribution { allocations, .. } => {
                references.extend(allocations.iter().filter_map(|allocation| {
                    match allocation.recipient {
                        RuntimeDamageRecipient::Object { object } => Some(object),
                        RuntimeDamageRecipient::Leader { .. } => None,
                    }
                }));
            }
            _ => {}
        }
    }
    for reference in references {
        if let Entry::Vacant(entry) = execution
            .processing_disclosure
            .subjects
            .entry((reference.id.0, reference.generation.0))
            && let Some(subject) = SubjectEvidence::capture(game, reference)
        {
            entry.insert(subject);
        }
    }
}

pub(in crate::game) fn observed_processing(
    game: &Game,
    perspective: Viewer,
) -> super::super::Result<Vec<ProcessingContext>> {
    // A fault retains exactly the typed stop that the authority kept or restored.
    if game.state.game["ended"] == true {
        return Ok(vec![]);
    }
    let mut contexts = vec![];
    if let Some(execution) = &game.typed {
        append_contexts(game, execution, perspective, true, &mut contexts)?;
    }
    Ok(contexts)
}

pub(in crate::game) fn public_number_declaration(game: &Game) -> bool {
    let Some(execution) = game
        .typed
        .as_ref()
        .filter(|execution| execution.nested.is_none())
    else {
        return false;
    };
    let Some((
        _,
        Wait::Integer {
            declaration: Some(receipt),
            ..
        },
    )) = &execution.waiting
    else {
        return false;
    };
    execution
        .plan
        .as_ref()
        .is_some_and(|plan| plan.has_unconditional_number(*receipt))
        && [Viewer::P1, Viewer::P2].into_iter().all(|viewer| {
            execution
                .processing_disclosure
                .viewers
                .get(&viewer)
                .is_some_and(|disclosure| disclosure.stage == Some(ProcessingStage::Resolve))
        })
}

fn append_contexts(
    game: &Game,
    execution: &Execution,
    perspective: Viewer,
    top: bool,
    contexts: &mut Vec<ProcessingContext>,
) -> super::super::Result<()> {
    if let Some(nested) = &execution.nested {
        append_contexts(game, &nested.parent, perspective, false, contexts)?;
    }
    let Some(disclosure) = execution.processing_disclosure.viewers.get(&perspective) else {
        return Ok(());
    };
    let Some(controller) = viewer(execution.context.controller) else {
        return Ok(());
    };
    // Quota admission belongs to its rule context, not this play's private progress.
    let active = top
        && execution.waiting.as_ref().is_some_and(|(question, wait)| {
            !matches!(wait, Wait::TriggerQuota { .. })
                && viewer(question.actor) == Some(perspective)
        });
    let stage = if active {
        match execution.question_stage {
            Some(QuestionStage::Target) => Some(ProcessingStage::Target),
            Some(QuestionStage::Allocate) => Some(ProcessingStage::Allocate),
            Some(QuestionStage::Pay) => Some(ProcessingStage::Pay),
            Some(QuestionStage::Resolve) => Some(ProcessingStage::Resolve),
            None => execution.play_phase.map(|phase| match phase {
                PlayPhase::Declare => ProcessingStage::Declare,
                PlayPhase::Pay => ProcessingStage::Pay,
                PlayPhase::Resolve => ProcessingStage::Resolve,
            }),
        }
    } else {
        disclosure.stage
    };
    let boundary = disclosure.payments.unwrap_or_default();
    let mut payments = completed(execution, boundary, PaymentBoundary::default())?;
    // Payers retain their own receipts across seat changes; a public play does not
    // authorize another player's later private payments.
    payments.extend(
        completed(execution, PaymentBoundary::capture(execution), boundary)?
            .into_iter()
            .filter(|payment| payment.actor == perspective),
    );
    contexts.push(ProcessingContext {
        stage,
        controller,
        actor: active.then_some(perspective),
        source: disclosure
            .origin
            .as_ref()
            .and_then(|origin| origin.observed(game, perspective)),
        subject: disclosure
            .subject
            .as_ref()
            .and_then(|subject| subject.observed(game, perspective)),
        procedure: active
            .then(|| {
                execution
                    .processing_disclosure
                    .procedures
                    .observed(perspective)
            })
            .flatten(),
        declared: declarations(game, execution, perspective, disclosure.stage.is_some()),
        payments,
    });
    Ok(())
}

fn subject(
    game: &Game,
    execution: &Execution,
    perspective: Viewer,
    recipient: RuntimeDamageRecipient,
) -> Option<ProcessingSubject> {
    match recipient {
        RuntimeDamageRecipient::Leader { player } => viewer(player).map(ProcessingSubject::Leader),
        RuntimeDamageRecipient::Object { object } => execution
            .processing_disclosure
            .subjects
            .get(&(object.id.0, object.generation.0))?
            .observed(game, perspective),
    }
}

fn declarations(
    game: &Game,
    execution: &Execution,
    perspective: Viewer,
    public: bool,
) -> Vec<ProcessingDeclaration> {
    let mut declared = Vec::new();
    for declaration in &execution.play_declarations {
        let entry = match declaration {
            PlayDeclaration::X { player, value }
                if public || viewer(*player) == Some(perspective) =>
            {
                viewer(*player).map(|actor| ProcessingDeclaration::X {
                    actor,
                    value: value.0,
                })
            }
            PlayDeclaration::Modes { player, modes, .. }
                if public || viewer(*player) == Some(perspective) =>
            {
                viewer(*player).map(|actor| ProcessingDeclaration::Modes {
                    actor,
                    modes: modes.iter().map(|mode| mode.0).collect(),
                })
            }
            PlayDeclaration::OptionalCost {
                player,
                required,
                accepted,
                ..
            } if public || viewer(*player) == Some(perspective) => {
                viewer(*player).map(|actor| ProcessingDeclaration::OptionalCost {
                    actor,
                    required: *required,
                    accepted: *accepted,
                })
            }
            PlayDeclaration::Recipients {
                player, members, ..
            } if public || viewer(*player) == Some(perspective) => members
                .iter()
                .map(|member| subject(game, execution, perspective, *member))
                .collect::<Option<Vec<_>>>()
                .and_then(|targets| {
                    Some(ProcessingDeclaration::Targets {
                        actor: viewer(*player)?,
                        targets,
                    })
                }),
            PlayDeclaration::Distribution {
                player,
                allocations,
                ..
            } if public || viewer(*player) == Some(perspective) => allocations
                .iter()
                .map(|allocation| {
                    Some((
                        subject(game, execution, perspective, allocation.recipient)?,
                        allocation.amount.0,
                    ))
                })
                .collect::<Option<Vec<_>>>()
                .and_then(|allocations| {
                    Some(ProcessingDeclaration::Distribution {
                        actor: viewer(*player)?,
                        allocations,
                    })
                }),
            _ => None,
        };
        declared.extend(entry);
    }
    for declaration in &execution.target_declarations {
        if !public && viewer(declaration.player) != Some(perspective) {
            continue;
        }
        if let Some(targets) = declaration
            .objects
            .iter()
            .map(|object| {
                subject(
                    game,
                    execution,
                    perspective,
                    RuntimeDamageRecipient::Object { object: *object },
                )
            })
            .collect::<Option<Vec<_>>>()
            && let Some(actor) = viewer(declaration.player)
        {
            declared.push(ProcessingDeclaration::Targets { actor, targets });
        }
    }
    // Number declarations are public actions: Fact::Declared projects to both seats unfiltered.
    declared.extend(
        execution
            .number_declarations
            .iter()
            .filter_map(|declaration| {
                Some(ProcessingDeclaration::Number {
                    actor: viewer(declaration.player)?,
                    value: declaration.value.0,
                })
            }),
    );
    declared
}
