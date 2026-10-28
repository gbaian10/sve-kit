//! Candidate-only lowering of C0's expanded tree, without source or runtime admission.

mod ast;
mod capture;
mod conditions;
mod costs;
mod current;
mod discard;
mod evolution;
mod exchange;
mod grant;
mod hand_payment;
mod keywords;
mod material;
mod numeric;
mod optional_payment;
mod ordering;
mod payment;
mod play;
mod quick;
mod search;
mod selection;
mod tokens;

use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    Arithmetic, BatchPolicy, Binding, Bool, CardProgram, Cardinality, DamageAmount, DamageFlags,
    EventPattern, EventSnapshotPolicy, Expr, FaceId, Instruction, InstructionContext,
    InstructionId, Int, IntOperation, KeywordId, Located, Nat, NatRead, Node, ObjectFrozen,
    ObjectOperand, ObjectView, PlayerRef, PlayerRelation, PlayerSet, Predicate, Program, ReceiptId,
    RecipientSet, ScopeId, SelectionView, Selector, SourceSpan, StepwiseMode, TextQuantifier,
    TriggerAggregation, TriggerCondition, TriggerSpec, WellFormedCardProgram, ZoneKind,
};
pub use current::{
    CardKeywordSourceUse, CurrentCardProgram, CurrentKeywordEvidence, CurrentSourceCheck,
    CurrentSourceEvidence, KeywordDeclarationPayload, MetadataObligation, NonBodySource,
    PrintedKeyword, PrintedKeywordsObligation,
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
    /// Intrinsic evolution, evolve-time, fanfare, lastword, attack-time, End Begin,
    /// field activation with ordered constant PP/Act/self-to-cemetery costs,
    /// draw, uniform damage to all leaders, destroy, banish, Act/Stand,
    /// controller leader life Add/Set, numeric modifications, constant continuous
    /// installations, Search, successful EX discounts and
    /// observed deck ordering have closed supported subsets. D1 separately checks
    /// whole-card runtime support; this is not source or qualification admission.
    /// Unfiltered field targets, same-play Distribution and a sole top-level
    /// `ChoiceKeyword` have closed supported subsets. Spell headers require
    /// [`Self::lower_with_printed_costs`]; this method rejects missing printed costs.
    /// Card keys remain candidate coordinates, not snapshot IDs.
    ///
    /// # Errors
    /// Rejects schema violations and any unsupported construct anywhere in the tree.
    pub fn lower(
        &self,
        tree: &Value,
    ) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
        self.lower_with_printed_costs(tree, &BTreeMap::new())
    }

    /// Lower candidates with base-face printed PP costs supplied by the snapshot caller.
    ///
    /// Keys are the same candidate coordinates as `tree`. These values do not prove
    /// identity or source admission; D1 must still match them against its catalog.
    /// Missing Spell costs are rejected, never inferred from author text.
    ///
    /// # Errors
    /// Rejects unsupported trees, absent Spell costs and invalid expanded input.
    pub fn lower_with_printed_costs(
        &self,
        tree: &Value,
        printed_costs: &BTreeMap<String, Nat>,
    ) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
        self.lower_coordinates(tree, printed_costs, None)
    }

    /// Consume only `CardAccepted.lowering_input()` from the current Python reader.
    ///
    /// This is an internal trusted boundary, not a public program encoding or a
    /// replacement for source reconstruction, use qualification or runtime admission.
    /// The reader currently rejects token sources; a nonempty token selection is
    /// rejected here too, rather than selecting a template by display name.
    ///
    /// # Errors
    /// Rejects absent/ambiguous identities, unknown current owners, token selections
    /// without a supported source contract and every unsupported expanded construct.
    pub fn lower_current(&self, input: &Value) -> Result<CurrentCardProgram, LoweringError> {
        current::lower(self, input)
    }

    /// Reconcile a complete trusted C0 handoff before attempting executable lowering.
    ///
    /// Success preserves declaration provenance but grants no runtime support. C0
    /// remains responsible for original author bytes and current registry admission.
    ///
    /// # Errors
    /// Rejects malformed wire, source or author mismatches and unconsumed obligations.
    pub fn check_current(&self, input: &Value) -> Result<CurrentSourceCheck, LoweringError> {
        current::check(self, input)
    }

    fn lower_coordinates(
        &self,
        tree: &Value,
        printed_costs: &BTreeMap<String, Nat>,
        identity: Option<&current::CurrentIdentity>,
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
            if identity.is_none() && !card.faces.is_empty() {
                return Err(unsupported(&path));
            }
            let coordinates = CardCoordinates {
                key: &key,
                identity,
            };
            let entries = ability_entries(card, &path)?;
            let mut allocator = Allocator {
                next: 0,
                next_scope: u64::try_from(entries.len())
                    .ok()
                    .and_then(|count| count.checked_mul(3))
                    .ok_or_else(|| unsupported(&path))?,
            };
            let mut abilities = Vec::new();
            let mut modifiers = Vec::new();
            let mut exchange = None;
            let mut payment_result = None;
            for (entry_path, ability) in &entries {
                if let Some(play) = &ability.play {
                    costs::lower(
                        &coordinates,
                        entry_path,
                        ability,
                        play,
                        &mut allocator,
                        &mut modifiers,
                    )?;
                    let source = source_span(&coordinates, entry_path, ability)?;
                    if let Some(lowered) = exchange::lower(play, &source, &mut allocator)? {
                        if exchange.is_some() {
                            return Err(unsupported(entry_path));
                        }
                        payment_result = play
                            .optional_cost
                            .as_ref()
                            .and_then(|cost| cost.result.clone())
                            .map(|name| (name, lowered.node.receipt));
                        exchange = Some(lowered);
                    }
                }
            }
            let play_faces =
                play_faces(&modifiers, exchange.as_ref()).ok_or_else(|| unsupported(&path))?;
            for (index, (entry_path, ability)) in entries.into_iter().enumerate() {
                if ability.play.is_some() {
                    continue;
                }
                if let Some(keywords) = &ability.keywords
                    && !matches!(keywords.as_slice(), [keyword] if keyword == "quick")
                {
                    abilities.extend(keywords::lower(
                        &mut allocator,
                        &coordinates,
                        &entry_path,
                        &ability,
                        keywords,
                    )?);
                    continue;
                }
                optional_payment::check_entry(&ability, exchange.is_some(), &entry_path)?;
                abilities.push(allocator.ability(
                    &coordinates,
                    &entry_path,
                    &ability,
                    entry_scope(index, &entry_path)?,
                    match identity {
                        Some(identity) => {
                            identity.printed_cost(&ability.sources[0].json(), &entry_path)?
                        }
                        None => printed_costs.get(&key).copied(),
                    },
                    payment_result.clone().filter(|_| {
                        source_span(&coordinates, &entry_path, &ability)
                            .is_ok_and(|source| play_faces.contains(&source.face))
                    }),
                )?);
            }
            let checked = check_card(
                CardProgram {
                    abilities,
                    card_play_costs: face_play_costs(modifiers, exchange),
                },
                &path,
            )?;
            cards.insert(key, checked);
        }
        Ok(cards)
    }
}

fn entry_scope(index: usize, path: &str) -> Result<ScopeId, LoweringError> {
    u64::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(3))
        .and_then(|index| index.checked_add(1))
        .map(ScopeId)
        .ok_or_else(|| unsupported(path))
}

/// Face play costs and their payment receipt belong to one printed face.
fn play_faces(
    modifiers: &[Located<super::PlayCostModifier>],
    exchange: Option<&Located<super::PlayCostExchange>>,
) -> Option<BTreeSet<String>> {
    let faces = modifiers
        .iter()
        .map(|item| item.source.face.clone())
        .chain(exchange.map(|item| item.source.face.clone()))
        .collect::<BTreeSet<_>>();
    (faces.len() <= 1).then_some(faces)
}

fn face_play_costs(
    modifiers: Vec<Located<super::PlayCostModifier>>,
    exchange: Option<Located<super::PlayCostExchange>>,
) -> Vec<Located<super::FacePlayCost>> {
    modifiers
        .first()
        .map(|first| first.source.clone())
        .or_else(|| exchange.as_ref().map(|item| item.source.clone()))
        .map_or_else(Vec::new, |source| {
            vec![Located {
                node: super::FacePlayCost {
                    face: FaceId(source.face.clone()),
                    modifiers,
                    exchange,
                },
                source,
            }]
        })
}

fn check_card(card: CardProgram, path: &str) -> Result<WellFormedCardProgram, LoweringError> {
    WellFormedCardProgram::check(card).map_err(|failure| {
        error(
            "ill_formed",
            failure
                .source
                .as_ref()
                .map_or(path, |source| source.author_path.as_str()),
        )
    })
}

fn ability_entries(
    card: ast::Card,
    path: &str,
) -> Result<Vec<(String, ast::Ability)>, LoweringError> {
    card.abilities
        .into_iter()
        .enumerate()
        .map(|(index, value)| (format!("{path}/abilities/{index}"), value))
        .chain(card.faces.into_iter().flat_map(|(face, container)| {
            container
                .abilities
                .into_iter()
                .enumerate()
                .map(move |(index, value)| {
                    (
                        format!("{path}/faces/{}/abilities/{index}", pointer_key(&face)),
                        value,
                    )
                })
        }))
        .map(|(entry_path, value)| {
            let ability: ast::Ability = decode(value, &entry_path)?;
            if !matches!(
                (&ability.sources[0], ability.keywords.is_some()),
                (ast::SourceUse::CardKeyword(_), true)
                    | (ast::SourceUse::Ability(_), false)
                    | (ast::SourceUse::Direct(_), _)
            ) {
                return Err(unsupported(&entry_path));
            }
            Ok((entry_path, ability))
        })
        .collect()
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
        key: &CardCoordinates<'_>,
        path: &str,
        entry: &ast::Ability,
        scope: ScopeId,
        printed_cost: Option<Nat>,
        payment_result: Option<(String, ReceiptId)>,
    ) -> Result<Located<Ability>, LoweringError> {
        if entry.imports.is_some() {
            return Err(unsupported(path));
        }

        if entry.evolve.is_some() {
            return evolution::lower(self, key, path, entry, scope);
        }
        if let Some(keywords) = &entry.keywords {
            return quick::lower(key, path, entry, scope, keywords);
        }
        let active_zones = active_zones(entry, path)?;
        let (body, head, keyword) = ability_body(entry, path)?;
        let source = source_span(key, path, entry)?;
        let ability_source = ability_source(&source, keyword);
        let mut context = BodyContext::new(entry, scope, body, payment_result, self);
        context.prepare_parameters(body, path)?;
        let optional =
            context.optional_payment(body, &source, &ability_source, &format!("{path}/{head}"))?;
        let mut program =
            context.body(body, &format!("{path}/{head}"), &source, &ability_source)?;
        if let Some(payment) = optional {
            program.steps.insert(0, payment.node);
            context.parameters.extend(payment.parameters);
        }
        let (kind, mut play, trigger) = if entry.spell.is_some() {
            if body.per_turn.is_some() {
                return Err(unsupported(path));
            }
            let cost = printed_cost.ok_or_else(|| unsupported(path))?;
            (
                AbilityKind::Spell,
                Some(payment::lower(
                    &mut context,
                    &ast::Cost::Short(ast::CostOperation::Pp(cost.0)),
                    &ability_source,
                    path,
                )?),
                None,
            )
        } else if let Some(activated) = &entry.activate {
            if body
                .per_turn
                .is_some_and(|maximum| !matches!(maximum, 1 | 2))
            {
                return Err(unsupported(path));
            }
            (
                AbilityKind::Activated,
                Some(payment::lower(
                    &mut context,
                    &activated.cost,
                    &ability_source,
                    path,
                )?),
                None,
            )
        } else {
            let event_id = context.allocator.id(path)?;
            let mut trigger = lowered_trigger(entry, keyword, event_id, scope, path)?;
            if body.per_turn.is_some_and(|maximum| maximum != 1) {
                return Err(unsupported(path));
            }
            trigger.limit = body.per_turn.map(|_| super::LimitSpec::once_per_turn());
            (
                AbilityKind::Triggered,
                (body.optional_cost.is_some() || body.head_cost.is_some()).then(|| {
                    super::PlaySpec {
                        limit: None,
                        captures: vec![],
                        parameters: vec![],
                        costs: super::CostSpec::Ordered { steps: vec![] },
                    }
                }),
                Some(trigger),
            )
        };
        if let Some(play) = &mut play {
            context.parameters.append(&mut play.parameters);
            play.parameters = context.parameters;
            if entry.activate.is_some() {
                play.limit = body
                    .per_turn
                    .map(|maximum| super::LimitSpec::activated_per_turn(Nat(maximum)));
            }
        }
        Ok(Located {
            source,
            node: Ability {
                id: AbilityId(scope.0),
                scope,
                source: ability_source,
                kind,
                qualification: AbilityQualification::Ordinary,
                active_zones,
                event_captures: vec![],
                play,
                trigger,
                body: Some(program),
                declarations: vec![],
            },
        })
    }
}

fn active_zones(entry: &ast::Ability, path: &str) -> Result<Vec<ZoneKind>, LoweringError> {
    let zones = match entry.activate.as_ref().and_then(|head| head.zone.as_ref()) {
        Some(ast::ActivationZones::One(zone)) => vec![zone.kind()],
        Some(ast::ActivationZones::Many(zones)) => zones.iter().map(|zone| zone.kind()).collect(),
        None if entry.spell.is_some() => vec![ZoneKind::Resolution],
        None => vec![ZoneKind::Field],
    };
    if entry.activate.is_some()
        && (!matches!(zones.as_slice(), [ZoneKind::Field | ZoneKind::Ex])
            || (zones == [ZoneKind::Ex]
                && entry
                    .activate
                    .as_ref()
                    .is_some_and(|head| head.quick == Some(true))))
    {
        return Err(unsupported(path));
    }
    Ok(zones)
}

fn ability_source(source: &SourceSpan, keyword: Option<KeywordId>) -> AbilitySource {
    let printed = AbilityOrigin::Printed {
        face: FaceId(source.face.clone()),
        line: Nat(u64::from(source.line)),
    };
    let origin = keyword.map_or_else(
        || printed.clone(),
        |keyword| AbilityOrigin::Keyword {
            keyword,
            origin: Box::new(printed.clone()),
        },
    );
    AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin,
        controller: PlayerRef::Controller {},
    }
}

fn ability_body<'entry>(
    entry: &'entry ast::Ability,
    path: &str,
) -> Result<(&'entry ast::Body, &'static str, Option<KeywordId>), LoweringError> {
    if let Some(body) = &entry.super_evolve_time {
        return Ok((body, "super_evolve_time", Some(KeywordId::OnSuperEvolve)));
    }
    Ok(
        match (
            &entry.fanfare,
            &entry.lastword,
            &entry.attack_time,
            &entry.auto,
            &entry.evolve_time,
            &entry.activate,
            &entry.spell,
        ) {
            (Some(body), None, None, None, None, None, None) => {
                (body, "fanfare", Some(KeywordId::Fanfare))
            }
            (None, Some(body), None, None, None, None, None) => {
                (body, "lastword", Some(KeywordId::Lastword))
            }
            (None, None, Some(body), None, None, None, None) => {
                (body, "attack_time", Some(KeywordId::OnAttack))
            }
            (None, None, None, Some(auto), None, None, None) => (&auto.body, "auto", None),
            (None, None, None, None, Some(body), None, None) => {
                (body, "evolve_time", Some(KeywordId::OnEvolve))
            }
            (None, None, None, None, None, Some(activated), None) => (
                &activated.body,
                "activate",
                (activated.quick == Some(true)).then_some(KeywordId::Quick),
            ),
            (None, None, None, None, None, None, Some(body)) => (body, "spell", None),
            _ => return Err(unsupported(path)),
        },
    )
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
        } else if matches!(
            keyword,
            Some(KeywordId::OnEvolve | KeywordId::OnSuperEvolve)
        ) {
            let event = Binding::new(event_id, scope);
            EventPattern::Evolve {
                binding: event,
                predicate: if keyword == Some(KeywordId::OnSuperEvolve) {
                    super::events::timing::super_evolve_predicate(event)
                } else {
                    super::events::timing::evolve_predicate(event)
                },
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
            active_zones: vec![ZoneKind::Field],
            view: if matches!(pattern, EventPattern::LeaveField { .. }) {
                EventSnapshotPolicy::BeforeExit
            } else {
                EventSnapshotPolicy::AfterEntry
            },
            on: TriggerCondition::Event { pattern },
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
                predicate: super::events::timing::phase_predicate(
                    event,
                    match phase.which {
                        ast::SubscribedPhase::Main => super::Phase::Main,
                        ast::SubscribedPhase::End => super::Phase::End,
                    },
                    player,
                ),
            })
        }
        ast::PhaseSubscription::LeaveField(entry) => {
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::LeaveField {
                binding: event,
                predicate: super::events::timing::leave_predicate(event, entry.during.is_some()),
            })
        }
        ast::PhaseSubscription::Damage(entry) => {
            let event = Binding::new(event_id, scope);
            let filter = super::events::timing::DamageFilter {
                ability: entry.kind.is_some(),
                own_turn: entry
                    .during
                    .as_ref()
                    .map(|during| matches!(during.turn, ast::EntryPlayer::Controller)),
            };
            Ok(EventPattern::Damage {
                binding: event,
                predicate: filter.predicate(event),
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
    local_payment: Option<(String, ReceiptId)>,
    definition: AbilityId,
    payment_result: Option<(String, ReceiptId)>,
    allocator: Allocator,
    card_allocator: &'definitions mut Allocator,
    definitions: &'definitions BTreeMap<String, ast::Number>,
    searches: BTreeMap<String, (ReceiptId, bool)>,
    observations: BTreeMap<String, super::ObjectReceipt>,
    targets: BTreeMap<String, Binding<Selector>>,
    recipient_targets: BTreeMap<String, Binding<Vec<super::DamageRecipient>>>,
    parameters: Vec<super::Decision>,
    resolution_scope: ScopeId,
    resolution_captures: Vec<super::Capture>,
    formal_play: bool,
    mode_targets: BTreeMap<u64, Vec<ast::Target>>,
}
impl<'definitions> BodyContext<'definitions> {
    fn new(
        entry: &ast::Ability,
        scope: ScopeId,
        body: &'definitions ast::Body,
        payment_result: Option<(String, ReceiptId)>,
        card_allocator: &'definitions mut Allocator,
    ) -> Self {
        Self {
            scope,
            definition: AbilityId(scope.0),
            payment_result: if entry.spell.is_some() {
                payment_result
            } else {
                None
            },
            allocator: Allocator::default(),
            card_allocator,
            definitions: &body.define,
            searches: BTreeMap::new(),
            observations: BTreeMap::new(),
            targets: BTreeMap::new(),
            recipient_targets: BTreeMap::new(),
            parameters: vec![],
            resolution_scope: scope,
            resolution_captures: vec![],
            mode_targets: BTreeMap::new(),
            formal_play: entry.activate.is_some()
                || entry.spell.is_some()
                || body.optional_cost.is_some()
                || body.head_cost.is_some(),
            local_payment: None,
        }
    }

    fn prepare_parameters(&mut self, body: &ast::Body, path: &str) -> Result<(), LoweringError> {
        for value in body.define.values() {
            self.number(value, path, &mut BTreeSet::new())?;
        }
        self.resolution_captures.clear();
        self.allocator = Allocator::default();
        let choice = play::choice_body(body, path)?;
        self.targets(&body.targets, choice, path)
    }

    fn body(
        &mut self,
        body: &ast::Body,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Program, LoweringError> {
        let mut program = self.steps(&body.steps, &format!("{path}/do"), source, ability_source)?;
        program.steps.splice(
            0..0,
            self.resolution_captures.drain(..).map(|capture| Located {
                source: source.clone(),
                node: Node::Capture { capture },
            }),
        );
        Ok(program)
    }

    fn steps(
        &mut self,
        author_steps: &[ast::Step],
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Program, LoweringError> {
        let mut steps = Vec::new();
        for (index, step) in author_steps.iter().enumerate() {
            let step_path = format!("{path}/{index}");
            if let ast::Step::If(conditional) = step {
                let condition = self.condition(&conditional.cond, source, &step_path)?;
                let parent = self.scope;
                self.scope = self.card_allocator.scope(&step_path)?;
                let then_body = self.steps(
                    &conditional.then,
                    &format!("{step_path}/if/then"),
                    source,
                    ability_source,
                )?;
                let else_body = if let Some(otherwise) = &conditional.r#else {
                    self.scope = self.card_allocator.scope(&step_path)?;
                    Some(self.steps(
                        otherwise,
                        &format!("{step_path}/if/else"),
                        source,
                        ability_source,
                    )?)
                } else {
                    None
                };
                self.scope = parent;
                let mut located = source.clone();
                located.author_path = step_path;
                steps.push(Located {
                    source: located,
                    node: Node::Control {
                        control: super::Control::If {
                            condition,
                            then_body,
                            else_body,
                        },
                    },
                });
                continue;
            }
            // The ability entry admits one top-level group; mode bodies reject nested choices.
            if let ast::Step::Choice(choice) = step {
                steps.push(self.choice(choice, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Choose(choose) = step {
                steps.push(self.choose(choose, &step_path, source)?);
                continue;
            }
            let instruction_id = self.card_allocator.id(&step_path)?;
            let action = self.action(step, &step_path, ability_source, source)?;
            self.remember(step, ReceiptId(instruction_id), &step_path)?;
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
        Ok(Program {
            scope: self.scope,
            steps,
        })
    }

    fn action(
        &mut self,
        step: &ast::Step,
        path: &str,
        source: &AbilitySource,
        span: &SourceSpan,
    ) -> Result<Action, LoweringError> {
        match step {
            ast::Step::Choice(_) | ast::Step::Choose(_) | ast::Step::If(_) => {
                Err(unsupported(path))
            }
            ast::Step::Destroy(value) => Ok(Action::Destroy {
                objects: self.action_objects(value, path)?,
            }),
            ast::Step::Banish(value) => Ok(Action::Banish {
                objects: self.action_objects(value, path)?,
            }),
            ast::Step::Act(value) | ast::Step::Stand(value) => Ok(Action::Posture {
                objects: self.action_objects(value, path)?,
                posture: if matches!(step, ast::Step::Act(_)) {
                    super::Posture::Acted
                } else {
                    super::Posture::Standing
                },
            }),
            ast::Step::Buff(value) => self.modify(value, false, path),
            ast::Step::Grant(value) => self.grant(value, span, path),
            ast::Step::SetStat(value) => self.modify(value, true, path),
            ast::Step::Apply(value) => self.continuous(value, source, path),
            ast::Step::Search(value) => self.search(value, path),
            ast::Step::Reveal(value) => self.reveal(value, path),
            ast::Step::Create(value) => self.create(value, path),
            ast::Step::Look(value) => {
                let ast::SelfPlayer::Controller = value.top.who;
                Ok(Action::Look {
                    viewer: PlayerRef::Controller {},
                    zone: super::ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Deck,
                    },
                    count: Expr::Literal(Nat(value.top.n)),
                    mode: StepwiseMode::Exact,
                })
            }
            ast::Step::Move(value) => self.observed_move(value, path),
            ast::Step::Pp(value) => {
                numeric::resource_increase(value, super::Resource::PpCurrent, path)
            }
            ast::Step::Discard(value) => self.discard(value, path),
            ast::Step::PpMax(value) => {
                numeric::resource_increase(value, super::Resource::PpMaximum, path)
            }
            ast::Step::Ep(value) => numeric::resource_increase(value, super::Resource::Ep, path),
            ast::Step::Draw(draw) => Ok(Action::Draw {
                player: PlayerRef::Controller {},
                count: self.number(&draw.n, path, &mut BTreeSet::new())?,
                mode: StepwiseMode::Exact,
            }),
            ast::Step::Damage(damage) => self.damage(damage, span, path),
            ast::Step::Life(life) => {
                let ast::SelfPlayer::Controller = life.who;
                Ok(Action::Life {
                    player: PlayerRef::Controller {},
                    change: super::NumericChange::Add {
                        amount: self.number(&life.delta, path, &mut BTreeSet::new())?,
                    },
                })
            }
            ast::Step::SetLife(life) => {
                let ast::SelfPlayer::Controller = life.who;
                Ok(Action::Life {
                    player: PlayerRef::Controller {},
                    change: super::NumericChange::Set {
                        amount: self.number(&life.n, path, &mut BTreeSet::new())?,
                    },
                })
            }
        }
    }
}

struct CardCoordinates<'input> {
    key: &'input str,
    identity: Option<&'input current::CurrentIdentity>,
}

fn source_span(
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
) -> Result<SourceSpan, LoweringError> {
    if let ast::SourceUse::Direct(source) = &entry.sources[0] {
        return current::direct::source_span(key, path, entry, source);
    }
    let source = entry.sources[0].json();
    let text = |pointer| {
        source
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| unsupported(path))
    };
    let (card_id, face) = key.identity.map_or_else(
        || Ok((key.key.to_owned(), "base".to_owned())),
        |identity| identity.owner(&source, path),
    )?;
    if key.identity.is_some()
        && path.contains("/faces/")
        && !path.starts_with(&format!(
            "/cards/{}/faces/{}/abilities/",
            pointer_key(key.key),
            pointer_key(&face)
        ))
    {
        return Err(error("invalid_current_identity", path));
    }
    Ok(SourceSpan {
        card_id,
        card_no: key.key.to_owned(),
        scope: match &entry.sources[0] {
            ast::SourceUse::Direct(direct_source) => direct_source.scope.clone(),
            ast::SourceUse::Ability(scoped) => scoped.ability_scope.clone(),
            ast::SourceUse::CardKeyword(_) => format!("keyword_declaration:{path}"),
        },
        owner: source
            .pointer("/occurrence/owner")
            .ok_or_else(|| unsupported(path))?
            .to_string(),
        face,
        field: text("/occurrence/field")?,
        section: entry.section.map(|section| section.to_string()),
        line: entry.line,
        fragment: Some(text("/binding_id")?),
        author_path: path.to_owned(),
    })
}
