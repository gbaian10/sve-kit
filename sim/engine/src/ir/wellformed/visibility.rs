use super::{Checked, Checker, Key};
use crate::ir::{
    Action, CaptureTime, Cardinality, ChoiceDomain, Instruction, Nat, PlayerRef, PlayerRelation,
    PlayerSet, ReceiptLocation, ReceiptReference, Selector, Visibility,
};
use crate::ir::{
    Binding, Control, CostSpec, Declaration, Node, ObjectReceipt, Program, RepeatBody,
    TextQuantifier,
};
use core::any::TypeId;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Observation provenance shares the structural checker environment."
)]
impl Checker {
    pub(super) fn visible(&self, access: Option<&Visibility>) -> Checked {
        if self.env.chooser.is_none() || access == Some(&Visibility::Public {}) {
            return Ok(());
        }
        if let Some(Visibility::Only { players }) = access
            && self.env.private_choice
            && self.env.chooser.as_ref() == Some(&players.relative_to)
            && players.relation == PlayerRelation::SelfPlayer
        {
            return Ok(());
        }
        self.reject()
    }

    /// Frozen scalars keep their evidence; objects may have been flipped since it was granted.
    pub(super) fn visible_object(&self, access: Option<&Visibility>) -> Checked {
        self.visible(access.filter(|_| !self.revoked))
    }

    pub(super) fn visible_receipt<T>(&self, receipt: &ReceiptReference<T>) -> Checked {
        match receipt.location() {
            ReceiptLocation::Local { id } => self.visible_object(self.env.receipt_access.get(id)),
            ReceiptLocation::Iteration { binding } => {
                self.visible_object(self.env.access.get(&binding.key()))
            }
        }
    }

    /// A later iteration follows every flip in the body, including one after a choice.
    pub(super) fn revoke_before_loop(&mut self, body: &Program) {
        if revokes(body) {
            self.revoked = true;
        }
    }

    pub(super) fn save_access(&mut self, key: Key, access: Option<Visibility>) {
        if let Some(access) = access {
            self.env.access.insert(key, access);
        }
    }

    pub(super) fn capture_access(
        &self,
        at: CaptureTime,
        check: impl Fn(&mut Self) -> Checked,
    ) -> Option<Visibility> {
        let mut probe = self.clone();
        if matches!(at, CaptureTime::AtResolution { .. }) {
            probe.env = self.resolution_entry.clone();
        }
        probe.infer_access(check)
    }

    pub(super) fn infer_access(&self, check: impl Fn(&mut Self) -> Checked) -> Option<Visibility> {
        let mut actors = vec![
            PlayerRef::Controller {},
            PlayerRef::Owner {},
            PlayerRef::Active {},
        ];
        for access in self
            .env
            .access
            .values()
            .chain(self.env.receipt_access.values())
        {
            if let Visibility::Only { players } = access
                && !actors.contains(&players.relative_to)
            {
                actors.push(players.relative_to.clone());
            }
        }
        for (key, slot) in &self.env.bindings {
            if slot.value == TypeId::of::<PlayerRef>() {
                actors.push(PlayerRef::Bound {
                    binding: Binding::new(key.1, key.0),
                });
            }
        }
        for private in [false, true] {
            for actor in &actors {
                let mut probe = self.clone();
                probe.env.chooser = Some(actor.clone());
                probe.env.private_choice = private;
                if check(&mut probe).is_ok() {
                    return Some(if private {
                        only(actor.clone())
                    } else {
                        Visibility::Public {}
                    });
                }
            }
        }
        None
    }

    pub(super) fn answer_access(&mut self, domain: &ChoiceDomain, visibility: &Visibility) {
        match domain {
            ChoiceDomain::Objects {
                binding,
                target_roles,
                ..
            } => {
                self.env.access.insert(binding.key(), visibility.clone());
                if let Some(roles) = target_roles {
                    for role in &roles.roles {
                        self.env
                            .access
                            .insert(role.binding.key(), visibility.clone());
                    }
                }
            }
            ChoiceDomain::Integer { binding, .. } => {
                self.env.access.insert(binding.key(), visibility.clone());
            }
            ChoiceDomain::Compound { domains } => {
                for child in domains {
                    self.answer_access(child, visibility);
                }
            }
            ChoiceDomain::EndPhaseGuardAct { .. }
            | ChoiceDomain::Modes { .. }
            | ChoiceDomain::Order { .. }
            | ChoiceDomain::Distribution { .. }
            | ChoiceDomain::Names { .. }
            | ChoiceDomain::QuickActionOrPass { .. } => {}
        }
    }

    fn reveal_access(&mut self, objects: &Selector) {
        match objects {
            Selector::Bound { reference } => {
                self.env
                    .access
                    .insert(reference.key(), Visibility::Public {});
            }
            Selector::Captured { reference } => {
                self.env
                    .access
                    .insert(reference.binding.key(), Visibility::Public {});
            }
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { receipt },
                ..
            } => {
                if let ReceiptLocation::Local { id } = receipt.location() {
                    self.env.receipt_access.insert(*id, Visibility::Public {});
                }
            }
            Selector::CollectedMembers { .. }
            | Selector::MoveAfterAll { .. }
            | Selector::Created { .. }
            | Selector::ReceiptMembers { .. }
            | Selector::MoveAfter { .. }
            | Selector::Zone { .. }
            | Selector::Filter { .. }
            | Selector::Union { .. }
            | Selector::Intersection { .. }
            | Selector::Difference { .. }
            | Selector::Ordered { .. }
            | Selector::Objects { .. }
            | Selector::SearchFound { .. }
            | Selector::SearchRevealed { .. }
            | Selector::Relation { .. } => {}
        }
    }

    pub(super) fn movement_input_access(&self, action: &Action) -> Option<Visibility> {
        if let Action::Move { objects, .. }
        | Action::Destroy { objects }
        | Action::Banish { objects } = action
        {
            self.infer_access(|checker| checker.selector(objects))
        } else {
            None
        }
    }

    pub(super) fn observation(&mut self, instruction: &Instruction) {
        match &instruction.action {
            Action::Look { viewer, .. } => {
                self.env
                    .receipt_access
                    .insert(instruction.receipt, only(viewer.clone()));
            }
            Action::Reveal { objects } if instruction.precondition.is_none() => {
                self.reveal_access(objects);
            }
            Action::Token { output, .. } => {
                self.env.access.insert(output.key(), Visibility::Public {});
                self.env
                    .receipt_access
                    .insert(instruction.receipt, Visibility::Public {});
            }
            Action::Dice { result, .. } => {
                self.env.access.insert(result.key(), Visibility::Public {});
            }
            Action::Declare { declaration, .. } => match declaration {
                Declaration::NonnegativeInteger { binding } => {
                    self.env.access.insert(binding.key(), Visibility::Public {});
                }
                Declaration::BoundedInteger { binding, .. } => {
                    self.env.access.insert(binding.key(), Visibility::Public {});
                }
                Declaration::CardNames { .. } => {}
            },
            Action::FlipDown { .. } => self.revoked = true,
            Action::Reveal { .. }
            | Action::Move { .. }
            | Action::Destroy { .. }
            | Action::Banish { .. }
            | Action::InstallEffect { .. }
            | Action::Discard { .. }
            | Action::Transfer { .. }
            | Action::Damage { .. }
            | Action::Life { .. }
            | Action::Draw { .. }
            | Action::Search { .. }
            | Action::RevealUntil { .. }
            | Action::Shuffle { .. }
            | Action::Order { .. }
            | Action::Evolve { .. }
            | Action::Transform { .. }
            | Action::Counter { .. }
            | Action::FlipUp { .. }
            | Action::Posture { .. }
            | Action::Resource { .. }
            | Action::Modify { .. }
            | Action::Grant { .. }
            | Action::RemoveAbility { .. }
            | Action::Delay { .. }
            | Action::PlayCard { .. }
            | Action::PlayAbility { .. }
            | Action::MealRace { .. }
            | Action::RideDrive { .. }
            | Action::DriveCheck { .. }
            | Action::Equip { .. }
            | Action::Fusion { .. }
            | Action::Random { .. }
            | Action::ScheduleTurn { .. }
            | Action::GameResult { .. }
            | Action::BoxPilot { .. } => {}
        }
    }
}

fn revokes(program: &Program) -> bool {
    program.steps.iter().any(|step| match &step.node {
        Node::Instruction { instruction } => flips(instruction),
        Node::Control { control } => match control {
            Control::Simultaneous { batch, .. } => {
                batch.members.iter().any(|member| flips(&member.node))
            }
            Control::Sequence { body }
            | Control::May { body, .. }
            | Control::EachObject { body, .. }
            | Control::Repeat {
                body: RepeatBody::Program { body },
                ..
            } => revokes(body),
            Control::If {
                then_body,
                else_body,
                ..
            } => revokes(then_body) || else_body.as_ref().is_some_and(revokes),
            Control::Mode { modes, .. } => modes.iter().any(|mode| revokes(&mode.body)),
            Control::EachPlayer { stages, .. } => stages.iter().any(|stage| revokes(&stage.body)),
            Control::Pay { costs, on_paid, .. } => pays_by_flipping(costs) || revokes(on_paid),
            Control::Match {
                leader_body,
                follower_body,
                ..
            } => revokes(leader_body) || revokes(follower_body),
            Control::Repeat { .. } => true,
        },
        Node::Capture { .. } | Node::Decision { .. } => false,
    })
}

fn pays_by_flipping(costs: &CostSpec) -> bool {
    match costs {
        CostSpec::Ordered { steps } => steps.iter().any(|step| flips(&step.instruction)),
        CostSpec::Alternative { alternatives } => alternatives.iter().any(pays_by_flipping),
    }
}

const fn flips(instruction: &Instruction) -> bool {
    matches!(instruction.action, Action::FlipDown { .. })
}

pub(super) const fn only(player: PlayerRef) -> Visibility {
    Visibility::Only {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: player,
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
    }
}

pub(super) fn private_to(visibility: &Visibility, actor: &PlayerRef) -> bool {
    matches!(visibility, Visibility::Only { players }
        if players.relation == PlayerRelation::SelfPlayer && &players.relative_to == actor)
}
