//! Candidate-only lowering of C0's expanded tree, without source or runtime admission.

mod ast;

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
    /// Only fanfare, lastword, draw, uniform damage to all leaders, and their numeric subset
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
            let mut allocator = Allocator::default();
            let mut abilities = Vec::new();
            for (index, value) in card.abilities.into_iter().enumerate() {
                let path = format!("{path}/abilities/{index}");
                let ability = decode(value, &path)?;
                let scope = u64::try_from(index)
                    .ok()
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
}
impl Allocator {
    fn id(&mut self, path: &str) -> Result<u64, LoweringError> {
        self.next = self.next.checked_add(1).ok_or_else(|| unsupported(path))?;
        Ok(self.next)
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

        let (body, head, keyword) = match (&entry.fanfare, &entry.lastword) {
            (Some(body), None) => (body, "fanfare", KeywordId::Fanfare),
            (None, Some(body)) => (body, "lastword", KeywordId::Lastword),
            _ => return Err(unsupported(path)),
        };
        let source = source_span(key, path, entry)?;
        let origin = AbilityOrigin::Keyword {
            keyword,
            origin: Box::new(AbilityOrigin::Printed {
                face: FaceId(source.face.clone()),
                line: Nat(u64::from(entry.line)),
            }),
        };
        let ability_source = AbilitySource {
            holder: ObjectOperand::SourceHolder {},
            origin,
            controller: PlayerRef::Controller {},
        };
        let mut context = BodyContext {
            scope,
            allocator: Self::default(),
            definitions: &body.define,
        };
        for value in body.define.values() {
            context.number(value, path, &mut BTreeSet::new())?;
        }
        context.allocator = Self::default();
        let mut steps = Vec::new();
        for (index, step) in body.steps.iter().enumerate() {
            let step_path = format!("{path}/{head}/do/{index}");
            let action = context.action(step, &step_path)?;
            let instruction_id = self.id(&step_path)?;
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
        let trigger = if keyword == KeywordId::Lastword {
            TriggerSpec::lastword(Binding::new(event_id, scope))
        } else {
            let event = Binding::new(event_id, scope);
            TriggerSpec {
                on: TriggerCondition::Event {
                    pattern: EventPattern::EnterField {
                        binding: event,
                        predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        })),
                    },
                },
                active_zones: vec![ZoneKind::Field],
                view: EventSnapshotPolicy::AfterEntry,
                limit: None,
                aggregation: TriggerAggregation::PerFact {},
            }
        };
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

struct BodyContext<'definitions> {
    scope: ScopeId,
    allocator: Allocator,
    definitions: &'definitions BTreeMap<String, ast::Number>,
}
impl BodyContext<'_> {
    fn action(&mut self, step: &ast::Step, path: &str) -> Result<Action, LoweringError> {
        match step {
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
