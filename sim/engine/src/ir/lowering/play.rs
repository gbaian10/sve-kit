use super::super::{
    ChoiceDomain, Control, Decision, DecisionId, DecisionKind, Disjointness, ExecutionRef,
    InputMode, Mode, ModeChoiceKind, ModeDecision, ModeDecisionKind, ModeId, PlaySlot,
    SelectionConstraint, SelectionGroup, SelectionSnapshot, Visibility,
};
use super::{
    AbilitySource, Action, Allocator, BTreeMap, BTreeSet, BatchPolicy, Binding, BodyContext, Bool,
    Cardinality, DamageAmount, DamageFlags, Expr, Int, Located, LoweringError, Nat, Node,
    ObjectView, PlayerRef, PlayerRelation, PlayerSet, RecipientSet, SelectionView, Selector,
    SourceSpan, TextQuantifier, ZoneKind, ast, unsupported,
};

const fn parameter(id: u64, slot: PlaySlot, domain: ChoiceDomain) -> Decision {
    Decision {
        id: DecisionId(id),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain,
        kind: DecisionKind::PlayParameter {
            slot,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Formal play shares the ability's binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn targets(
        &mut self,
        targets: &[ast::Target],
        choice: bool,
        path: &str,
    ) -> Result<(), LoweringError> {
        if !targets.is_empty() && !self.formal_play {
            return Err(unsupported(path));
        }
        let mut names = BTreeSet::new();
        for target in targets {
            if self.definitions.contains_key(&target.bind) || !names.insert(&target.bind) {
                return Err(unsupported(path));
            }
            match (choice, target.option) {
                (true, Some(option)) => self
                    .mode_targets
                    .entry(option)
                    .or_default()
                    .push(target.clone()),
                (false, None) => self.target(target, path)?,
                _ => return Err(unsupported(path)),
            }
        }
        Ok(())
    }

    fn target(&mut self, target: &ast::Target, path: &str) -> Result<(), LoweringError> {
        let cardinality = match (target.n, target.upto) {
            (Some(n), None) => Cardinality {
                minimum: Nat(n),
                maximum: Some(Nat(n)),
            },
            (None, Some(n)) => Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(n)),
            },
            _ => return Err(unsupported(path)),
        };
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        if self.definitions.contains_key(&target.bind)
            || self.targets.insert(target.bind.clone(), binding).is_some()
        {
            return Err(unsupported(path));
        }
        let candidate = Binding::new(1, self.card_allocator.scope(path)?);
        let selector = Selector::Zone {
            players: PlayerSet {
                relation: match target.from.who {
                    ast::Side::Controller => PlayerRelation::SelfPlayer,
                    ast::Side::Opponent => PlayerRelation::Opponents,
                    ast::Side::All => PlayerRelation::All,
                },
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
                text_quantifier: if matches!(target.from.who, ast::Side::All) {
                    TextQuantifier::All
                } else {
                    TextQuantifier::Implicit
                },
            },
            zone: match target.from.zone {
                ast::Zone::Field => ZoneKind::Field,
            },
            view: SelectionView::Current {},
            binding: candidate,
            predicate: Box::new(Expr::Literal(Bool(true))),
        };
        self.parameters.push(parameter(
            self.card_allocator.id(path)?,
            PlaySlot::Targets,
            ChoiceDomain::Objects {
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector,
                        cardinality,
                    }],
                    cardinality,
                    distinct_by: None,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
                binding,
                target_roles: None,
            },
        ));
        Ok(())
    }

    pub(super) fn damage(
        &mut self,
        damage: &ast::Damage,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let recipients = match &damage.to {
            ast::Recipients::Leaders {
                leaders: ast::AllPlayers::All,
            } => RecipientSet::Leaders {
                players: PlayerSet {
                    relation: PlayerRelation::All,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::All,
                },
            },
            ast::Recipients::Target(name) => RecipientSet::Objects {
                objects: Selector::Bound {
                    reference: *self.targets.get(name).ok_or_else(|| unsupported(path))?,
                },
            },
        };
        let value = self.number(&damage.n, path, &mut BTreeSet::new())?;
        let amount = if damage.distribute == Some(true) {
            if !self.formal_play
                || self.targets.len() != 1
                || self.parameters.len() != 1
                || !matches!(&damage.to, ast::Recipients::Target(_))
            {
                return Err(unsupported(path));
            }
            let Expr::Literal(Int(total)) = value else {
                return Err(unsupported(path));
            };
            let total = u64::try_from(total).map_err(|_private| unsupported(path))?;
            let binding = Binding::new(self.allocator.id(path)?, self.scope);
            self.parameters.push(parameter(
                self.card_allocator.id(path)?,
                PlaySlot::Distribution,
                ChoiceDomain::Distribution {
                    recipients: recipients.clone(),
                    total: Expr::Literal(Nat(total)),
                    minimum: Nat(1),
                    binding,
                },
            ));
            DamageAmount::Allocation { binding }
        } else {
            if damage.distribute_at.is_some() {
                return Err(unsupported(path));
            }
            DamageAmount::Uniform { amount: value }
        };
        Ok(Action::Damage {
            recipients,
            amount,
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        })
    }

    pub(super) fn choice(
        &mut self,
        choice: &ast::Choice,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        if !self.formal_play
            || choice.n == 0
            || choice.n
                > u64::try_from(choice.options.len()).map_err(|_private| unsupported(path))?
        {
            return Err(unsupported(path));
        }
        let maximum = u64::try_from(choice.options.len()).map_err(|_private| unsupported(path))?;
        if self.mode_targets.keys().any(|id| *id == 0 || *id > maximum) {
            return Err(unsupported(path));
        }
        let mut modes = Vec::new();
        for (index, steps) in choice.options.iter().enumerate() {
            let mode_id = u64::try_from(index)
                .ok()
                .and_then(|i| i.checked_add(1))
                .ok_or_else(|| unsupported(path))?;
            let scope = self.card_allocator.scope(path)?;
            let mut context = BodyContext {
                scope,
                allocator: Allocator::default(),
                card_allocator: self.card_allocator,
                definitions: self.definitions,
                searches: BTreeMap::new(),
                observations: BTreeMap::new(),
                targets: BTreeMap::new(),
                parameters: vec![],
                formal_play: false,
                mode_targets: BTreeMap::new(),
            };
            if let Some(targets) = self.mode_targets.get(&mode_id) {
                for target in targets {
                    context.target(target, path)?;
                }
            }
            let body = context.steps(
                steps,
                &format!("{path}/choice/options/{index}"),
                source,
                ability_source,
            )?;
            modes.push(Mode {
                id: ModeId(mode_id),
                parameters: context.parameters,
                body,
            });
        }
        let id = self.card_allocator.id(path)?;
        let chosen = ModeDecision {
            id: DecisionId(id),
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Modes {
                modes: modes.iter().map(|mode| mode.id).collect(),
                cardinality: Cardinality {
                    minimum: Nat(choice.n),
                    maximum: Some(Nat(choice.n)),
                },
                binding: Binding::new(self.allocator.id(path)?, self.scope),
                exclude_used: None,
            },
            kind: ModeDecisionKind::PlayParameter {
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            visibility: Visibility::Public {},
        };
        let mut source = source.clone();
        path.clone_into(&mut source.author_path);
        Ok(Located {
            source,
            node: Node::Control {
                control: Control::Mode {
                    modes,
                    count: Expr::Literal(Nat(choice.n)),
                    chosen,
                    choice_kind: ModeChoiceKind::ChoiceKeyword,
                },
            },
        })
    }
}
