//! Candidate-only lowering of C0's expanded tree, without source or runtime admission.

mod ast;
mod evolution;
mod numeric;

use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    Arithmetic, BatchPolicy, Binding, Bool, CardProgram, Cardinality, DamageAmount, DamageFlags,
    EventPattern, EventSnapshotPolicy, Expr, FaceId, Instruction, InstructionContext,
    InstructionId, Int, IntOperation, KeywordId, Located, Nat, NatRead, Node, ObjectFrozen,
    ObjectOperand, ObjectView, PlayerRef, PlayerRelation, PlayerSet, Predicate, Program, ReceiptId,
    RecipientSet, ScopeId, SelectionView, Selector, SourceSpan, StepwiseMode, TextQuantifier,
    TriggerAggregation, TriggerCondition, TriggerSpec, WellFormedCardProgram, ZoneKind,
};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// A code and author JSON pointer; parser diagnostics never expose input values.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {path}")]
pub struct LoweringError {
    /// `unsupported_construct`, `invalid_expanded`, `invalid_schema`, or `ill_formed`.
    pub code: &'static str,
    /// RFC 6901 pointer in the candidate tree.
    pub path: String,
}

fn error(code: &'static str, path: &str) -> LoweringError {
    LoweringError {
        code,
        path: path.to_owned(),
    }
}
fn unsupported(path: &str) -> LoweringError {
    error("unsupported_construct", path)
}
fn decode<T>(value: Value, path: &str) -> Result<T, LoweringError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|_private| unsupported(path))
}
fn pointer_key(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Reusable expanded-profile validator and minimal candidate compiler.
#[derive(Debug)]
pub struct ExpandedLowerer {
    validator: jsonschema::Validator,
}
impl ExpandedLowerer {
    /// Compile the shared schema without changing its dynamic-anchor scope.
    ///
    /// # Errors
    /// Returns `invalid_schema` if the bundled resource cannot be compiled.
    pub fn new() -> Result<Self, LoweringError> {
        Ok(Self {
            validator: validator("expanded")?,
        })
    }

    /// Lower every card atomically. Success proves only internal IR well-formedness.
    ///
    /// Only intrinsic evolution, evolve-time, fanfare, lastword, attack-time, End Begin,
    /// draw, uniform damage to all leaders, destroy, numeric modifications,
    /// and constant continuous installations with the supported turn durations
    /// are supported. Card keys remain candidate coordinates, not snapshot IDs.
    ///
    /// # Errors
    /// Rejects schema violations and any unsupported construct anywhere in the tree.
    pub fn lower(
        &self,
        tree: &Value,
    ) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
        self.validator
            .validate(tree)
            .map_err(|failure| error("invalid_expanded", &failure.instance_path().to_string()))?;
        let document: ast::Document = decode(tree.clone(), "")?;
        if document.scope != "shared" {
            return Err(unsupported("/scope"));
        }
        let mut cards = BTreeMap::new();
        for (key, card) in document.cards {
            let path = format!("/cards/{}", pointer_key(&key));
            let card: ast::Card = decode(card, &path)?;
            let mut allocator = Allocator {
                next: 0,
                next_scope: u64::try_from(card.abilities.len())
                    .ok()
                    .and_then(|count| count.checked_mul(3))
                    .ok_or_else(|| unsupported(&path))?,
            };
            let mut abilities = Vec::new();
            for (index, value) in card.abilities.into_iter().enumerate() {
                let path = format!("{path}/abilities/{index}");
                let ability = decode(value, &path)?;
                let scope = u64::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_mul(3))
                    .and_then(|index| index.checked_add(1))
                    .ok_or_else(|| unsupported(&path))?;
                abilities.push(allocator.ability(&key, &path, &ability, ScopeId(scope))?);
            }
            let checked = WellFormedCardProgram::check(CardProgram { abilities })
                .map_err(|_private| error("ill_formed", &path))?;
            cards.insert(key, checked);
        }
        Ok(cards)
    }
}

pub(super) fn validator(profile: &str) -> Result<jsonschema::Validator, LoweringError> {
    let schema: Value =
        serde_json::from_str(include_str!("../../../../dsl/author-nodes.schema.json"))
            .map_err(|_private| error("invalid_schema", ""))?;
    let registry = jsonschema::Registry::new()
        .add("urn:sve-kit:author-nodes:1.0", schema)
        .and_then(jsonschema::RegistryBuilder::prepare)
        .map_err(|_private| error("invalid_schema", ""))?;
    jsonschema::options()
        .with_registry(&registry)
        .build(&json!({"$ref": format!("urn:sve-kit:author-nodes:1.0#/$defs/{profile}")}))
        .map_err(|_private| error("invalid_schema", ""))
}

#[derive(Default)]
struct Allocator {
    next: u64,
    next_scope: u64,
}
impl Allocator {
    fn id(&mut self, path: &str) -> Result<u64, LoweringError> {
        self.next = self.next.checked_add(1).ok_or_else(|| unsupported(path))?;
        Ok(self.next)
    }

    fn scope(&mut self, path: &str) -> Result<ScopeId, LoweringError> {
        self.next_scope = self
            .next_scope
            .checked_add(1)
            .ok_or_else(|| unsupported(path))?;
        Ok(ScopeId(self.next_scope))
    }

    fn ability(
        &mut self,
        key: &str,
        path: &str,
        entry: &ast::Ability,
        scope: ScopeId,
    ) -> Result<Located<Ability>, LoweringError> {
        if entry.imports.is_some() {
            return Err(unsupported(path));
        }

        if entry.evolve.is_some() {
            return evolution::lower(self, key, path, entry, scope);
        }
        let (body, head, keyword) = match (
            &entry.fanfare,
            &entry.lastword,
            &entry.attack_time,
            &entry.auto,
            &entry.evolve_time,
        ) {
            (Some(body), None, None, None, None) => (body, "fanfare", Some(KeywordId::Fanfare)),
            (None, Some(body), None, None, None) => (body, "lastword", Some(KeywordId::Lastword)),
            (None, None, Some(body), None, None) => {
                (body, "attack_time", Some(KeywordId::OnAttack))
            }
            (None, None, None, Some(auto), None) => (&auto.body, "auto", None),
            (None, None, None, None, Some(body)) => {
                (body, "evolve_time", Some(KeywordId::OnEvolve))
            }
            _ => return Err(unsupported(path)),
        };
        let source = source_span(key, path, entry)?;
        let printed = AbilityOrigin::Printed {
            face: FaceId(source.face.clone()),
            line: Nat(u64::from(entry.line)),
        };
        let origin = keyword.map_or_else(
            || printed.clone(),
            |keyword| AbilityOrigin::Keyword {
                keyword,
                origin: Box::new(printed.clone()),
            },
        );
        let ability_source = AbilitySource {
            holder: ObjectOperand::SourceHolder {},
            origin,
            controller: PlayerRef::Controller {},
        };
        let mut context = BodyContext {
            scope,
            allocator: Self::default(),
            card_allocator: self,
            definitions: &body.define,
        };
        for value in body.define.values() {
            context.number(value, path, &mut BTreeSet::new())?;
        }
        context.allocator = Self::default();
        let mut steps = Vec::new();
        for (index, step) in body.steps.iter().enumerate() {
            let step_path = format!("{path}/{head}/do/{index}");
            let action = context.action(step, &step_path, &ability_source)?;
            let instruction_id = context.card_allocator.id(&step_path)?;
            let mut step_source = source.clone();
            step_source.author_path = step_path;
            steps.push(Located {
                source: step_source,
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(instruction_id),
                        receipt: ReceiptId(instruction_id),
                        actor: PlayerRef::Controller {},
                        source: ability_source.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action,
                    }),
                },
            });
        }
        let event_id = context.allocator.id(path)?;
        let mut trigger = lowered_trigger(entry, keyword, event_id, scope, path)?;
        if body.per_turn.is_some_and(|maximum| maximum != 1) {
            return Err(unsupported(path));
        }
        trigger.limit = body.per_turn.map(|_| super::LimitSpec::once_per_turn());
        Ok(Located {
            source,
            node: Ability {
                id: AbilityId(scope.0),
                scope,
                source: ability_source,
                kind: AbilityKind::Triggered,
                qualification: AbilityQualification::Ordinary,
                active_zones: vec![ZoneKind::Field],
                event_captures: vec![],
                play: None,
                trigger: Some(trigger),
                body: Some(Program { scope, steps }),
                declarations: vec![],
            },
        })
    }
}

fn lowered_trigger(
    entry: &ast::Ability,
    keyword: Option<KeywordId>,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<TriggerSpec, LoweringError> {
    Ok(if keyword == Some(KeywordId::Lastword) {
        TriggerSpec::lastword(Binding::new(event_id, scope))
    } else {
        let pattern = if keyword == Some(KeywordId::OnAttack) {
            let event = Binding::new(event_id, scope);
            EventPattern::Attack {
                binding: event,
                predicate: super::events::timing::attack_predicate(event),
            }
        } else if keyword == Some(KeywordId::OnEvolve) {
            let event = Binding::new(event_id, scope);
            EventPattern::Evolve {
                binding: event,
                predicate: super::events::timing::evolve_predicate(event),
            }
        } else if let Some(auto) = &entry.auto {
            lowered_subscription(&auto.on, event_id, scope, path)?
        } else {
            let event = Binding::new(event_id, scope);
            EventPattern::EnterField {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                    left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
                    right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                })),
            }
        };
        TriggerSpec {
            on: TriggerCondition::Event { pattern },
            active_zones: vec![ZoneKind::Field],
            view: EventSnapshotPolicy::AfterEntry,
            limit: None,
            aggregation: TriggerAggregation::PerFact {},
        }
    })
}

fn lowered_subscription(
    on: &ast::PhaseSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    match on {
        ast::PhaseSubscription::Phase(phase) => {
            let player = match phase.whose {
                ast::PhasePlayer::Controller => Some(PlayerRef::Controller {}),
                ast::PhasePlayer::Opponent => Some(super::events::timing::opponent()),
                ast::PhasePlayer::Each => None,
            };
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::Phase {
                binding: event,
                predicate: super::events::timing::end_predicate(event, player),
            })
        }
        ast::PhaseSubscription::EnterField(entry) => {
            let itself = entry.reference.as_ref().map(|_| true);
            let own = entry
                .who
                .as_ref()
                .map(|side| matches!(side, ast::EntryPlayer::Controller));
            if itself.is_none() && (own.is_none() || entry.sel.is_none()) {
                return Err(unsupported(path));
            }
            let filter = super::events::entry::EntryFilter {
                own,
                itself,
                follower: entry.sel.is_some(),
                trait_id: entry
                    .sel
                    .as_ref()
                    .and_then(|sel| sel.trait_id.as_ref())
                    .map(|id| super::TraitId(id.clone())),
                own_turn: entry.during.is_some(),
            };
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::EnterField {
                binding: event,
                predicate: filter.predicate(event),
            })
        }
    }
}

struct BodyContext<'definitions> {
    scope: ScopeId,
    allocator: Allocator,
    card_allocator: &'definitions mut Allocator,
    definitions: &'definitions BTreeMap<String, ast::Number>,
}
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Numeric lowering shares the ability binding allocator."
)]
impl BodyContext<'_> {
    fn action(
        &mut self,
        step: &ast::Step,
        path: &str,
        source: &AbilitySource,
    ) -> Result<Action, LoweringError> {
        match step {
            ast::Step::Destroy(value) => Ok(Action::Destroy {
                objects: match value {
                    ast::Destroy::Short(ast::This::This) => numeric::holder(),
                    ast::Destroy::Long { r#ref } => self.objects(r#ref, path)?,
                },
            }),
            ast::Step::Buff(value) => self.modify(value, false, path),
            ast::Step::SetStat(value) => self.modify(value, true, path),
            ast::Step::Apply(value) => self.continuous(value, source, path),
            ast::Step::Draw(draw) => Ok(Action::Draw {
                player: PlayerRef::Controller {},
                count: self.number(&draw.n, path, &mut BTreeSet::new())?,
                mode: StepwiseMode::Exact,
            }),
            ast::Step::Damage(damage) => {
                let ast::AllPlayers::All = damage.to.leaders;
                Ok(Action::Damage {
                    recipients: RecipientSet::Leaders {
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
                    amount: DamageAmount::Uniform {
                        amount: self.number(&damage.n, path, &mut BTreeSet::new())?,
                    },
                    flags: DamageFlags::Ability,
                    source: ObjectView::SourceInformation {},
                    batch: BatchPolicy::Instruction {},
                })
            }
        }
    }

    fn number(
        &mut self,
        value: &ast::Number,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        match value {
            ast::Number::Literal(value) => Ok(Expr::Literal(Int(*value))),
            ast::Number::Alias(name) => {
                if !active.insert(name.clone()) {
                    return Err(unsupported(path));
                }
                let definition = self
                    .definitions
                    .get(name)
                    .ok_or_else(|| unsupported(path))?;
                let result = self.number(definition, path, active);
                active.remove(name);
                result
            }
            ast::Number::Minus { minus } => {
                let [left, right] = minus.as_ref();
                Ok(Expr::Compute(Box::new(IntOperation::Arithmetic {
                    operation: Arithmetic::Subtract,
                    arguments: vec![
                        self.number(left, path, active)?,
                        self.number(right, path, active)?,
                    ],
                })))
            }
            ast::Number::Count { count } => {
                let ast::SelfPlayer::Controller = count.who;
                let ast::DeckZone::Deck = count.zone;
                Ok(Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Live(Box::new(NatRead::ZoneCount {
                        selector: Selector::Zone {
                            players: PlayerSet {
                                relation: PlayerRelation::SelfPlayer,
                                relative_to: PlayerRef::Controller {},
                                cardinality: Cardinality {
                                    minimum: Nat(1),
                                    maximum: Some(Nat(1)),
                                },
                                text_quantifier: TextQuantifier::Implicit,
                            },
                            zone: ZoneKind::Deck,
                            view: SelectionView::Current {},
                            binding: Binding::new(self.allocator.id(path)?, self.scope),
                            predicate: Box::new(Expr::Literal(Bool(true))),
                        },
                    })),
                })))
            }
        }
    }
}

fn source_span(key: &str, path: &str, entry: &ast::Ability) -> Result<SourceSpan, LoweringError> {
    let [source] = &entry.sources;
    let text = |pointer| {
        source
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| unsupported(path))
    };
    Ok(SourceSpan {
        card_id: key.to_owned(),
        card_no: key.to_owned(),
        scope: text("/ability_scope")?,
        owner: source
            .pointer("/occurrence/owner")
            .ok_or_else(|| unsupported(path))?
            .to_string(),
        face: "base".to_owned(),
        field: text("/occurrence/field")?,
        section: entry.section.map(|section| section.to_string()),
        line: entry.line,
        fragment: Some(text("/binding_id")?),
        author_path: path.to_owned(),
    })
}
