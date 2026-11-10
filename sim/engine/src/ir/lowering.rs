//! Candidate-only lowering of C0's expanded tree, without source or runtime admission.
use crate::ir::ride;
use ast::PpAmount;

mod ability_restrictions;
mod additional;
mod advance_entry;
mod aggregates;
mod ast;
mod attack_restrictions;
mod capture;
mod combat;
mod conditions;
mod costs;
mod counters;
mod current;
mod dice;
mod discard;
mod earth;
mod evolution;
mod exchange;
mod grant;
mod granted_play;
mod hand_exit;
mod hand_payment;
mod keywords;
mod material;
mod material_values;
mod movement;
mod name_aliases;
mod nested_ability;
mod numeric;
mod optional_payment;
mod ordering;
mod other_costs;
mod payment;
mod play;
mod play_restrictions;
mod prevention;
mod public_reveal;
mod receipt_counts;
mod recipient_union;
mod resource_entry;
mod reveal_cost;
mod reveal_until;
mod stack;
mod static_keywords;
mod trigger_restrictions;
mod x_pp;

mod flip;
mod fusion;
mod posture;
mod quick;
mod search;
mod selection;
mod shuffle;
mod tokens;
mod transform;

use super::events::subject::SubjectFields;
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    Arithmetic, BatchPolicy, Binding, Bool, CardProgram, Cardinality, DamageAmount, DamageFlags,
    EventPattern, EventSnapshotPolicy, Expr, FaceId, Instruction, InstructionContext,
    InstructionId, Int, IntOperation, KeywordId, Located, Nat, NatRead, Node, ObjectFrozen,
    ObjectOperand, ObjectView, PaymentReference, PlayerRef, PlayerRelation, PlayerSet, Predicate,
    Program, ReceiptId, RecipientSet, ScopeId, SelectionView, Selector, SourceSpan, StepwiseMode,
    TextQuantifier, TriggerCondition, TriggerSpec, WellFormedCardProgram, ZoneKind,
};
pub use current::{
    CardKeywordSourceUse, CurrentCardProgram, CurrentKeywordEvidence, CurrentSourceCheck,
    CurrentSourceEvidence, KeywordDeclarationPayload, MetadataObligation, NonBodySource,
    PrintedKeyword, PrintedKeywordsObligation,
};

use super::AbilityQualification::{Ordinary, UnionBurst};
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
            if identity.is_none() && (!card.faces.is_empty() || card.deck_rule.is_some()) {
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
            let LoweredPlayCosts {
                modifiers,
                exchange,
                additional,
                payment_result,
            } = lower_play_costs(&coordinates, &entries, &mut allocator, &path)?;
            let play_faces = play_faces(&modifiers, exchange.as_ref(), additional.as_ref())
                .ok_or_else(|| unsupported(&path))?;
            for (index, (entry_path, ability)) in entries.into_iter().enumerate() {
                if allocator.declarations(
                    &coordinates,
                    &entry_path,
                    &ability,
                    index,
                    &mut abilities,
                )? {
                    continue;
                }
                optional_payment::check_entry(&ability, exchange.is_some(), &entry_path)?;
                let branches = if hand_exit::check(&ability, &entry_path)? {
                    vec![
                        Some(hand_exit::Branch::Discard),
                        Some(hand_exit::Branch::Banish),
                    ]
                } else {
                    vec![None]
                };
                for (alternative, branch) in branches.into_iter().enumerate() {
                    let scope = if alternative == 0 {
                        entry_scope(index, &entry_path)?
                    } else {
                        allocator.granted_scope(&entry_path)?
                    };
                    abilities.push(allocator.ability(
                        &coordinates,
                        &entry_path,
                        hand_exit::AbilityInput {
                            entry: &ability,
                            branch,
                        },
                        scope,
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
            }
            let checked = check_card(
                CardProgram {
                    abilities,
                    card_play_costs: face_play_costs(modifiers, exchange, additional),
                },
                &path,
            )?;
            cards.insert(key, checked);
        }
        Ok(cards)
    }
}

struct LoweredPlayCosts {
    modifiers: Vec<Located<super::PlayCostModifier>>,
    exchange: Option<Located<super::PlayCostExchange>>,
    additional: Option<Located<super::PlayCostPayment>>,
    payment_result: Option<PaymentBindings>,
}

#[derive(Clone)]
struct PaymentBindings {
    complete: Option<(String, ReceiptId)>,
    discard: Option<(String, ReceiptId, Cardinality)>,
}

fn lower_play_costs(
    coordinates: &CardCoordinates<'_>,
    entries: &[(String, ast::Ability)],
    allocator: &mut Allocator,
    path: &str,
) -> Result<LoweredPlayCosts, LoweringError> {
    let mut modifiers = Vec::new();
    let mut exchange = None;
    let mut additional = None;
    let mut payment_result = None;
    for (entry_path, ability) in entries {
        if let Some(play) = &ability.play {
            costs::lower(
                coordinates,
                entry_path,
                ability,
                play,
                allocator,
                &mut modifiers,
            )?;
            if let Some(lowered) =
                additional::lower(ability, play, coordinates, entry_path, allocator)?
            {
                if additional.is_some() {
                    return Err(unsupported(entry_path));
                }
                payment_result = play
                    .additional_cost
                    .as_ref()
                    .and_then(|cost| cost.result.clone())
                    .map(|name| PaymentBindings {
                        complete: Some((name, lowered.node.receipt)),
                        discard: None,
                    });
                additional = Some(lowered);
            }
            let source = source_span(coordinates, entry_path, ability)?;
            if let Some(lowered) = exchange::lower(play, &source, allocator)? {
                if exchange.is_some() {
                    return Err(unsupported(entry_path));
                }
                payment_result = Some(exchange::bindings(
                    play.optional_cost
                        .as_ref()
                        .ok_or_else(|| unsupported(entry_path))?,
                    &lowered.node,
                    entry_path,
                )?);
                exchange = Some(lowered);
            }
        }
    }
    if additional.is_some() && exchange.is_some() {
        return Err(unsupported(path));
    }
    Ok(LoweredPlayCosts {
        modifiers,
        exchange,
        additional,
        payment_result,
    })
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
    additional: Option<&Located<super::PlayCostPayment>>,
) -> Option<BTreeSet<String>> {
    let faces = modifiers
        .iter()
        .map(|item| item.source.face.clone())
        .chain(exchange.map(|item| item.source.face.clone()))
        .chain(additional.map(|item| item.source.face.clone()))
        .collect::<BTreeSet<_>>();
    (faces.len() <= 1).then_some(faces)
}

fn face_play_costs(
    modifiers: Vec<Located<super::PlayCostModifier>>,
    exchange: Option<Located<super::PlayCostExchange>>,
    additional: Option<Located<super::PlayCostPayment>>,
) -> Vec<Located<super::FacePlayCost>> {
    modifiers
        .first()
        .map(|first| first.source.clone())
        .or_else(|| exchange.as_ref().map(|item| item.source.clone()))
        .or_else(|| additional.as_ref().map(|item| item.source.clone()))
        .map_or_else(Vec::new, |source| {
            vec![Located {
                node: super::FacePlayCost {
                    face: FaceId(source.face.clone()),
                    modifiers,
                    exchange,
                    additional,
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
    fn declarations(
        &mut self,
        key: &CardCoordinates<'_>,
        path: &str,
        entry: &ast::Ability,
        index: usize,
        abilities: &mut Vec<Located<Ability>>,
    ) -> Result<bool, LoweringError> {
        if let Some(declaration) = &entry.r#static {
            let lower = match declaration.effects.first() {
                Some(ast::StaticEffect::UnlimitedEvolve(_)) => evolution::unlimited,
                Some(ast::StaticEffect::Reroll(_)) => dice::reroll,
                Some(ast::StaticEffect::NoTrigger(_)) => trigger_restrictions::lower,
                Some(ast::StaticEffect::Grant(_)) => static_keywords::lower,
                Some(ast::StaticEffect::AlsoNamed(_)) => name_aliases::lower,
                Some(ast::StaticEffect::CostMod(_) | ast::StaticEffect::EvolveCostMod(_)) => {
                    other_costs::lower
                }
                Some(
                    ast::StaticEffect::Prevent(_)
                    | ast::StaticEffect::DamageMod(_)
                    | ast::StaticEffect::DamageCap(_),
                ) => prevention::lower,
                Some(ast::StaticEffect::If(_))
                    if declaration.effects.iter().any(|effect| {
                        matches!(effect, ast::StaticEffect::Cannot(ast::Cannot::Removal(_)))
                    }) =>
                {
                    prevention::lower
                }
                Some(ast::StaticEffect::Cannot(_) | ast::StaticEffect::IgnoreWard(_)) => {
                    attack_restrictions::lower
                }
                Some(ast::StaticEffect::If(_)) | None => return Err(unsupported(path)),
            };
            abilities.push(lower(self, key, path, entry, entry_scope(index, path)?)?);
            return Ok(true);
        }
        if let Some(play) = &entry.play {
            if let Some(permission) = &play.playable_from {
                if play.cannot_if.is_some()
                    || !play.define.is_empty()
                    || play.cost_delta.is_some()
                    || play.cost_set.is_some()
                    || play.additional_cost.is_some()
                    || play.optional_cost.is_some()
                    || play.optional_cost_delta.is_some()
                    || play.optional_cost_set.is_some()
                {
                    return Err(unsupported(path));
                }
                abilities.push(play_restrictions::cemetery(
                    self,
                    key,
                    path,
                    entry,
                    permission,
                    entry_scope(index, path)?,
                )?);
                return Ok(true);
            }
            if let Some(condition) = &play.cannot_if {
                abilities.push(play_restrictions::lower(
                    self,
                    key,
                    path,
                    entry,
                    condition,
                    entry_scope(index, path)?,
                )?);
            }
            return Ok(true);
        }
        if let Some(keywords) = &entry.keywords
            && !matches!(keywords.as_slice(), [keyword] if keyword == "quick")
        {
            abilities.extend(keywords::lower(self, key, path, entry, keywords)?);
            return Ok(true);
        }
        Ok(false)
    }

    fn id(&mut self, path: &str) -> Result<u64, LoweringError> {
        self.next = self.next.checked_add(1).ok_or_else(|| unsupported(path))?;
        Ok(self.next)
    }

    fn granted_scope(&mut self, path: &str) -> Result<ScopeId, LoweringError> {
        // Each entry reserves three scopes for its body and formal choices.
        const GRANTED_SCOPE_START: u64 = 1 << 63;
        self.scope(path)?
            .0
            .checked_mul(3)
            .and_then(|offset| offset.checked_add(GRANTED_SCOPE_START))
            .map(ScopeId)
            .ok_or_else(|| unsupported(path))
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
        input: hand_exit::AbilityInput<'_>,
        scope: ScopeId,
        printed_cost: Option<Nat>,
        payment_result: Option<PaymentBindings>,
    ) -> Result<Located<Ability>, LoweringError> {
        let entry = input.entry;
        if entry.imports.is_some() {
            return Err(unsupported(path));
        }

        if entry.evolve.is_some() {
            return evolution::lower(self, key, path, entry, scope);
        }
        if entry.feed.is_some() || entry.ride.is_some() {
            return resource_entry::lower(self, key, path, entry, scope);
        }
        if let Some(keywords) = &entry.keywords {
            return quick::lower(key, path, entry, scope, keywords);
        }
        let source = source_span(key, path, entry)?;
        self.executable_ability(input, scope, printed_cost, payment_result, source, path)
    }

    fn executable_ability(
        &mut self,
        input: hand_exit::AbilityInput<'_>,
        scope: ScopeId,
        printed_cost: Option<Nat>,
        payment_result: Option<PaymentBindings>,
        source: SourceSpan,
        path: &str,
    ) -> Result<Located<Ability>, LoweringError> {
        let entry = input.entry;
        if hand_exit::is_union(entry) != input.branch.is_some() {
            return Err(unsupported(path));
        }
        let active_zones = active_zones(entry, path)?;
        let (body, head, keyword) = ability_body(entry, path)?;
        let ability_source = ability_source(&source, keyword);
        let payment_result = payment_result.filter(|_| entry.spell.is_some());
        let face = FaceId(source.face.clone());
        let complete = payment_result
            .as_ref()
            .and_then(|binding| binding.complete.clone())
            .map(|(name, receipt)| (name, PaymentReference::CardPlay { face, receipt }));
        let mut context = BodyContext::new(entry, scope, body, complete, self);
        context.hand_exit = input.branch;
        if let Some((name, receipt, bounds)) = payment_result.and_then(|binding| binding.discard) {
            payment::record_discard(&mut context, &name, receipt, bounds, path)?;
        }
        let guard = context.activation_qualification(entry.activate.as_ref(), path)?;
        let trigger_choice = context.prepare_parameters(
            body,
            path,
            entry.activate.is_none() && entry.spell.is_none(),
        )?;
        let triggered_play = play::trigger_play_spec(entry, body, trigger_choice);
        let optional =
            context.optional_payment(body, &source, &ability_source, &format!("{path}/{head}"))?;
        let events = head_events(entry);
        if events.self_entry {
            context.entry_event = Some(Binding::new(context.allocator.id(path)?, scope));
        }
        if events.self_departure {
            context.leave_event = Some(Binding::new(context.allocator.id(path)?, scope));
        }
        context.departure_to_cemetery = events.departure_to_cemetery;
        context.source_can_enter_field =
            matches!(active_zones.as_slice(), [ZoneKind::Cemetery | ZoneKind::Ex]);
        let activated_play = entry
            .activate
            .as_ref()
            .map(|activated| payment::activated(&mut context, activated, &ability_source, path))
            .transpose()?;
        let prepared_trigger = context.prepare_event(entry, keyword, body, path)?;
        let mut program =
            context.body(body, &format!("{path}/{head}"), &source, &ability_source)?;
        if let Some(payment) = optional {
            program.steps.insert(0, payment.node);
            context.parameters.extend(payment.parameters);
        }
        if body
            .per_turn
            .is_some_and(|maximum| entry.spell.is_some() || maximum == 0)
        {
            return Err(unsupported(path));
        }
        let (kind, mut play, trigger) = if entry.spell.is_some() {
            let cost = printed_cost.ok_or_else(|| unsupported(path))?;
            (
                AbilityKind::Spell,
                Some(payment::lower(
                    &mut context,
                    &ast::Cost::Short(Box::new(ast::CostOperation::Pp(PpAmount::Literal(cost.0)))),
                    &ability_source,
                    path,
                )?),
                None,
            )
        } else if let Some(play) = activated_play {
            (AbilityKind::Activated, Some(play), None)
        } else {
            let trigger = prepared_trigger.ok_or_else(|| unsupported(path))?;
            (AbilityKind::Triggered, triggered_play, Some(trigger))
        };
        context.check_x(path)?;
        if let Some(play) = &mut play {
            conditions::qualify_costs(&mut play.costs, guard, path)?;
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
                qualification: entry_qualification(entry, body),
                active_zones,
                event_captures: context.event_captures,
                play,
                trigger,
                body: Some(program),
                declarations: vec![],
            },
        })
    }
}

fn entry_qualification(entry: &ast::Ability, body: &ast::Body) -> AbilityQualification {
    if entry
        .activate
        .as_ref()
        .is_some_and(|activation| activation.fusion.is_some())
    {
        AbilityQualification::Fusion
    } else {
        body.ub.as_ref().map_or(Ordinary, |_| UnionBurst)
    }
}

struct HeadEvents {
    self_entry: bool,
    self_departure: bool,
    departure_to_cemetery: bool,
}

fn head_events(entry: &ast::Ability) -> HeadEvents {
    let on = entry.auto.as_ref().map(|auto| &auto.on);
    HeadEvents {
        self_entry: entry.fanfare.is_some()
            || matches!(on, Some(ast::PhaseSubscription::EnterField(event)) if event.reference.is_some()),
        self_departure: entry.lastword.is_some()
            || matches!(on, Some(ast::PhaseSubscription::LeaveField(event)) if event.reference.is_some()),
        departure_to_cemetery: entry.lastword.is_some(),
    }
}

fn active_zones(entry: &ast::Ability, path: &str) -> Result<Vec<ZoneKind>, LoweringError> {
    let requested = entry
        .activate
        .as_ref()
        .and_then(|head| head.zone.as_ref())
        .or_else(|| entry.auto.as_ref()?.zone.as_ref());
    let zones = match requested {
        Some(ast::ActivationZones::One(zone)) => vec![zone.kind()],
        Some(ast::ActivationZones::Many(zones)) => zones.iter().map(|zone| zone.kind()).collect(),
        None if hand_exit::is_union(entry) || entry.auto.as_ref().is_some_and(|auto| matches!(&auto.on, ast::PhaseSubscription::Discarded(event) if event.reference.is_some())) => vec![ZoneKind::Hand],
        None if entry.spell.is_some() => vec![ZoneKind::Resolution],
        None if entry.activate.as_ref().is_some_and(|head| head.fusion.is_some()) => vec![ZoneKind::Hand],
        None => vec![ZoneKind::Field],
    };
    // Quick candidates do not pre-check EX room, so a hand card cannot offer itself there.
    if entry.activate.as_ref().is_some_and(|head| {
        head.quick == Some(true) && head.cost.as_ref().is_some_and(ast::Cost::moves_self_to_ex)
    }) {
        return Err(unsupported(path));
    }
    if entry.activate.is_some()
        && (!matches!(
            zones.as_slice(),
            [ZoneKind::Field | ZoneKind::Ex | ZoneKind::Hand | ZoneKind::Cemetery]
        ) || (zones == [ZoneKind::Ex]
            && entry
                .activate
                .as_ref()
                .is_some_and(|head| head.quick == Some(true))))
    {
        return Err(unsupported(path));
    }
    if let Some(auto) = &entry.auto {
        if matches!(auto.on, ast::PhaseSubscription::Equipped(_)) {
            return if zones == [ZoneKind::Equipment] {
                Ok(zones)
            } else {
                Err(unsupported(path))
            };
        }
        let cemetery = auto.cemetery_observer();
        // A card in the cemetery never observes its own entry.
        if cemetery
            && matches!(&auto.on, ast::PhaseSubscription::EnterField(subscription) if subscription.reference.is_some())
        {
            return Err(unsupported(path));
        }
        let hand_discard = zones == [ZoneKind::Hand]
            && (hand_exit::is_union(entry)
                || matches!(&auto.on, ast::PhaseSubscription::Discarded(event) if event.reference.is_some()));
        if !cemetery
            && !hand_discard
            && (!matches!(zones.as_slice(), [ZoneKind::Field | ZoneKind::Ex])
                || (zones == [ZoneKind::Ex]
                    && !matches!(auto.on, ast::PhaseSubscription::Phase(_))
                    // 「EXエリアにこれが置かれたとき」 observes only its own arrival in EX.
                    && !matches!(&auto.on, ast::PhaseSubscription::PlacedEx(placed)
                        if placed.reference.is_some())
                    && !matches!(&auto.on, ast::PhaseSubscription::Fused(_) | ast::PhaseSubscription::AnyOf(_))))
        {
            return Err(unsupported(path));
        }
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
    if let Some(activated) = &entry.activate
        && activated.advance.is_some()
    {
        if activated.quick.is_some() || activated.zone.is_some() {
            return Err(unsupported(path));
        }
        return Ok((
            &activated.body,
            "activate",
            Some(KeywordId::AdvanceActivate),
        ));
    }
    if let Some(body) = &entry.drive_time {
        return Ok((body, "drive_time", Some(KeywordId::OnDriveGain)));
    }
    if let Some(body) = &entry.race_time {
        return Ok((body, "race_time", Some(KeywordId::OnRace)));
    }
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

#[derive(Clone, Copy)]
struct EventIds {
    event: u64,
    subject: u64,
    facet: u64,
    scope: ScopeId,
}

fn lowered_trigger(
    entry: &ast::Ability,
    keyword: Option<KeywordId>,
    ids: EventIds,
    path: &str,
    allocator: &mut Allocator,
) -> Result<TriggerSpec, LoweringError> {
    let EventIds {
        event: event_id,
        subject: subject_id,
        scope,
        ..
    } = ids;
    Ok(if keyword == Some(KeywordId::Lastword) {
        TriggerSpec::lastword(Binding::new(event_id, scope))
    } else {
        let pattern = if keyword == Some(KeywordId::OnDriveGain) {
            let event = Binding::new(event_id, scope);
            EventPattern::DriveGained {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                    left: Expr::Frozen(Box::new(ObjectFrozen::DriveGainedSubject { event })),
                    right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                })),
            }
        } else if keyword == Some(KeywordId::OnRace) {
            let event = Binding::new(event_id, scope);
            EventPattern::Race {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                    left: Expr::Frozen(Box::new(ObjectFrozen::RaceSubject { event })),
                    right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                })),
            }
        } else if keyword == Some(KeywordId::OnAttack) {
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
            lowered_subscription(&auto.on, ids, path, allocator)?
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
            active_zones: active_zones(entry, path)?,
            view: if matches!(
                pattern,
                EventPattern::Play { .. } | EventPattern::UbActivated { .. }
            ) || super::events::fusion::combined(&pattern)
            {
                EventSnapshotPolicy::AtReveal
            } else if matches!(
                pattern,
                EventPattern::Fused { .. }
                    | EventPattern::LeaveField { .. }
                    | EventPattern::Destroy { .. }
                    | EventPattern::Discard { .. }
                    | EventPattern::Move { .. }
            ) {
                EventSnapshotPolicy::BeforeExit
            } else {
                EventSnapshotPolicy::AfterEntry
            },
            on: TriggerCondition::Event { pattern },
            limit: None,
            aggregation: discard::aggregation(entry.auto.as_ref(), subject_id, scope, path)?,
        }
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive event-header dispatch keeps each admitted subscription auditable together."
)]
fn lowered_subscription(
    on: &ast::PhaseSubscription,
    ids: EventIds,
    path: &str,
    allocator: &mut Allocator,
) -> Result<EventPattern, LoweringError> {
    let EventIds {
        event: event_id,
        subject: subject_id,
        facet: facet_id,
        scope,
    } = ids;
    match on {
        ast::PhaseSubscription::AnyOf(_) if hand_exit::is_hand_union(on) => {
            Ok(hand_exit::discard(event_id, scope))
        }
        ast::PhaseSubscription::Fused(_) | ast::PhaseSubscription::AnyOf(_) => {
            fusion::subscription(on, ids, path, allocator)
        }
        ast::PhaseSubscription::DiceRolled(value) => {
            let ast::SelfPlayer::Controller = value.who;
            let binding = Binding::new(event_id, scope);
            Ok(EventPattern::Dice {
                binding,
                predicate: super::dice::predicate(binding),
            })
        }
        ast::PhaseSubscription::Equipped(_) => {
            let binding = Binding::new(event_id, scope);
            Ok(EventPattern::Equip {
                binding,
                predicate: super::events::equipment::predicate(binding),
            })
        }
        ast::PhaseSubscription::UbActivated(_ub) => {
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::UbActivated {
                binding: event,
                predicate: super::events::ability::subject().predicate(SubjectFields::Ub(event)),
            })
        }
        ast::PhaseSubscription::DriveChecked(_) => {
            let binding = Binding::new(event_id, scope);
            Ok(EventPattern::DriveChecked {
                binding,
                predicate: super::events::drive::checked(binding),
            })
        }
        ast::PhaseSubscription::DriveTrigger(trigger) => {
            let binding = Binding::new(event_id, scope);
            Ok(EventPattern::DriveTrigger {
                binding,
                predicate: super::events::drive::triggered(binding, trigger.during.is_some()),
            })
        }
        ast::PhaseSubscription::Drew(draw) => {
            let ast::StartPhase::Start = draw.not_phase;
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::Draw {
                binding: event,
                predicate: super::events::draw::predicate(event),
            })
        }
        ast::PhaseSubscription::Discarded(event) => {
            discard::subscription(event, event_id, subject_id, scope, path)
        }
        ast::PhaseSubscription::Banished(_) => Err(unsupported(path)),
        ast::PhaseSubscription::Played(played) => {
            let event = Binding::new(event_id, scope);
            let card = Binding::new(subject_id, scope);
            movement::played(played, event, card, path)
        }
        ast::PhaseSubscription::Phase(phase) => {
            Ok(phase_subscription(phase, Binding::new(event_id, scope)))
        }
        ast::PhaseSubscription::LeaveField(entry)
            if entry.reference.is_some() && entry.who.is_none() && entry.sel.is_none() =>
        {
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::LeaveField {
                binding: event,
                predicate: super::events::timing::leave_predicate(event, entry.during.is_some()),
            })
        }
        ast::PhaseSubscription::LeaveField(_)
        | ast::PhaseSubscription::Destroyed(_)
        | ast::PhaseSubscription::Moved(_)
        | ast::PhaseSubscription::PlacedEx(_) => {
            movement::subscription(on, event_id, subject_id, scope, path)
        }
        ast::PhaseSubscription::Acted(_)
        | ast::PhaseSubscription::Stood(_)
        | ast::PhaseSubscription::StatChanged(_) => {
            lowered_change(on, event_id, subject_id, facet_id, scope, path)
        }
        ast::PhaseSubscription::Evolved(entry) => {
            evolution::subscription(entry, event_id, scope, path)
        }
        ast::PhaseSubscription::Attacked(entry) => combat::attack(entry, event_id, scope, path),
        ast::PhaseSubscription::Dealt(entry) => {
            combat::dealt(entry, event_id, subject_id, scope, path)
        }
        ast::PhaseSubscription::LifeChanged(entry) => Ok(combat::life(entry, event_id, scope)),
        ast::PhaseSubscription::Damage(entry) => Ok(combat::damaged(entry, event_id, scope)),
        ast::PhaseSubscription::EnterField(entry) => {
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::EnterField {
                binding: event,
                predicate: movement::movement_filter(
                    entry
                        .who
                        .as_ref()
                        .map(|side| matches!(side, ast::EntryPlayer::Controller)),
                    entry.reference.is_some(),
                    entry.sel.as_ref(),
                    entry.during.is_some(),
                    path,
                    movement::FilterSupport::EntryCost,
                )?
                .predicate(SubjectFields::Entry(event)),
            })
        }
    }
}

fn phase_subscription(
    phase: &ast::PhaseSubscriptionPayload,
    event: Binding<super::PhaseEvent>,
) -> EventPattern {
    let player = match phase.whose {
        ast::PhasePlayer::Controller => Some(PlayerRef::Controller {}),
        ast::PhasePlayer::Opponent => Some(super::events::timing::opponent()),
        ast::PhasePlayer::Each => None,
    };
    EventPattern::Phase {
        binding: event,
        predicate: super::events::timing::phase_predicate(
            event,
            match phase.which {
                ast::SubscribedPhase::Main => super::Phase::Main,
                ast::SubscribedPhase::End => super::Phase::End,
            },
            player,
        ),
    }
}

fn lowered_change(
    on: &ast::PhaseSubscription,
    event_id: u64,
    subject_id: u64,
    facet_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    match on {
        ast::PhaseSubscription::Acted(entry) | ast::PhaseSubscription::Stood(entry) => {
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::Posture {
                binding: event,
                predicate: super::events::change::PostureFilter {
                    subject: movement::movement_filter(
                        entry
                            .who
                            .as_ref()
                            .map(|who| matches!(who, ast::EntryPlayer::Controller)),
                        entry.reference.is_some(),
                        entry.sel.as_ref(),
                        false,
                        path,
                        movement::FilterSupport::Basic,
                    )?,
                    after: if matches!(on, ast::PhaseSubscription::Acted(_)) {
                        super::Posture::Acted
                    } else {
                        super::Posture::Standing
                    },
                }
                .predicate(event),
            })
        }
        ast::PhaseSubscription::StatChanged(entry) => {
            let event = Binding::new(event_id, scope);
            let properties = match &entry.stat {
                None => vec![super::Stat::Attack, super::Stat::Life],
                Some(ast::ChangeStats::One(stat)) => vec![match stat {
                    ast::ChangeStat::Power => super::Stat::Attack,
                    ast::ChangeStat::Hp => super::Stat::Life,
                }],
                Some(ast::ChangeStats::Many(stats)) => stats
                    .iter()
                    .map(|stat| match stat {
                        ast::ChangeStat::Power => super::Stat::Attack,
                        ast::ChangeStat::Hp => super::Stat::Life,
                    })
                    .collect(),
            };
            Ok(EventPattern::StatChange {
                binding: event,
                predicate: super::events::change::StatFilter {
                    itself: entry.reference.is_some(),
                    properties,
                    up: entry
                        .direction
                        .as_ref()
                        .map(|direction| matches!(direction, ast::LifeDirection::Up)),
                    subject: Binding::new(subject_id, scope),
                    facet: Binding::new(facet_id, scope),
                }
                .predicate(event),
            })
        }
        ast::PhaseSubscription::Fused(_)
        | ast::PhaseSubscription::AnyOf(_)
        | ast::PhaseSubscription::Evolved(_)
        | ast::PhaseSubscription::UbActivated(_)
        | ast::PhaseSubscription::Played(_)
        | ast::PhaseSubscription::Phase(_)
        | ast::PhaseSubscription::EnterField(_)
        | ast::PhaseSubscription::LeaveField(_)
        | ast::PhaseSubscription::Destroyed(_)
        | ast::PhaseSubscription::Moved(_)
        | ast::PhaseSubscription::PlacedEx(_)
        | ast::PhaseSubscription::Damage(_)
        | ast::PhaseSubscription::Attacked(_)
        | ast::PhaseSubscription::Dealt(_)
        | ast::PhaseSubscription::DriveChecked(_)
        | ast::PhaseSubscription::DriveTrigger(_)
        | ast::PhaseSubscription::Drew(_)
        | ast::PhaseSubscription::Discarded(_)
        | ast::PhaseSubscription::LifeChanged(_)
        | ast::PhaseSubscription::Equipped(_)
        | ast::PhaseSubscription::DiceRolled(_)
        | ast::PhaseSubscription::Banished(_) => Err(unsupported(path)),
    }
}

#[derive(Clone, Copy)]
enum CounterReadView {
    Current,
    SourceInformation,
}

/// Free field slots are read when a private multi-card pick is asked, so it may enter the field
/// only as the very next step of the same body.
#[derive(Default)]
struct FieldEntryGuard {
    multi: BTreeSet<String>,
    /// Binding of the choose step directly before the step being lowered.
    adjacent: Option<String>,
}

fn choice_before(steps: &[ast::Step], index: usize) -> Option<String> {
    if let Some(ast::Step::Choose(choice)) = index.checked_sub(1).and_then(|i| steps.get(i)) {
        Some(choice.bind.clone())
    } else {
        None
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Explicit plays and moves to EX introduce opponent cemetery selection authority."
)]
fn nested_targets(steps: &[ast::Step]) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    for step in steps {
        match step {
            ast::Step::PlayCard(play) => {
                targets.insert(play.r#ref.clone());
            }
            ast::Step::Move(movement)
                if matches!(movement.to, ast::Destination::Ex)
                    && let Some(ast::MoveObjects::Field(ast::Objects::Bound(name))) =
                        &movement.r#ref =>
            {
                targets.insert(name.clone());
            }
            ast::Step::Pay(payment) => targets.extend(nested_targets(&payment.steps)),
            ast::Step::Optional(optional) => {
                targets.extend(nested_targets(&optional.steps));
                if let Some(following_steps) = &optional.then {
                    targets.extend(nested_targets(following_steps));
                }
            }
            ast::Step::If(branch) => {
                targets.extend(nested_targets(&branch.then));
                if let Some(else_steps) = &branch.r#else {
                    targets.extend(nested_targets(else_steps));
                }
            }
            ast::Step::EachPlayer(each) => {
                for stage in &each.stages {
                    targets.extend(nested_targets(stage));
                }
            }
            ast::Step::Choice(choice) => {
                for option in &choice.options {
                    targets.extend(nested_targets(option));
                }
            }
            ast::Step::Either(either) => {
                for option in &either.options {
                    targets.extend(nested_targets(option));
                }
            }
            _ => {}
        }
    }
    targets
}

/// Receipts whose members' card information a body condition may read.
#[derive(Clone, Default)]
struct ReceiptReads {
    published_looks: BTreeSet<ReceiptId>,
    evolutions: BTreeMap<String, ReceiptId>,
    paid_materials: BTreeMap<String, ReceiptId>,
    paid_material_acts: BTreeMap<String, ReceiptId>,
    /// Declared bounds of named discard payments.
    discard_bounds: BTreeMap<ReceiptId, Cardinality>,
    /// Movements of cards leaving the deck, which carry no modifications.
    deck_moves: BTreeSet<ReceiptId>,
    deck_cemetery: BTreeSet<ReceiptId>,
}

#[derive(Clone)]
struct FieldOutput {
    receipt: super::MovementReceipt,
    single_evolution_subject: bool,
}

struct BodyContext<'definitions> {
    x: Option<x_pp::Parameter>,
    static_zone: Option<ZoneKind>,
    /// Only an activation printed in the hand may move its own card to EX as a cost.
    activation_zone: Option<ZoneKind>,
    subject_event: Option<SubjectFields>,
    damage_event: Option<Binding<super::DamageEvent>>,
    dice_event: Option<Binding<super::DiceEvent>>,
    dice_results: BTreeMap<String, Binding<Nat>>,
    equip_event: Option<Binding<super::EquipEvent>>,
    drive_checked_event: Option<Binding<super::DriveCheckedEvent>>,
    source_use: &'definitions ast::SourceUse,
    counter_read_view: CounterReadView,
    entry_event: Option<Binding<super::EnterFieldEvent>>,
    evolution: Option<&'definitions ast::Evolution>,
    source_can_enter_field: bool,
    departure_to_cemetery: bool,
    leave_event: Option<Binding<super::LeaveFieldEvent>>,
    discard_event: Option<Binding<super::DiscardEvent>>,
    banish_event: Option<Binding<super::BanishEvent>>,
    hand_exit: Option<hand_exit::Branch>,
    discard_pp_event: Option<Binding<super::DiscardEvent>>,
    scope: ScopeId,
    local_payment: Option<(String, ReceiptId)>,
    definition: AbilityId,
    payment_result: Option<(String, PaymentReference)>,
    allocator: Allocator,
    card_allocator: &'definitions mut Allocator,
    definitions: &'definitions BTreeMap<String, ast::Number>,
    players: BTreeMap<String, Binding<PlayerRef>>,
    searches: BTreeSet<String>,
    ex_outputs: BTreeMap<String, Selector>,
    field_outputs: BTreeMap<String, FieldOutput>,
    evolution_targets: BTreeSet<String>,
    ex_targets: BTreeSet<String>,
    flip_targets: BTreeSet<String>,
    hand_targets: BTreeSet<String>,
    playable_targets: BTreeSet<String>,
    opponent_cemetery_targets: BTreeSet<String>,
    cemetery_return_targets: BTreeSet<String>,
    nested_targets: BTreeSet<String>,
    observations: BTreeMap<String, super::ObjectReceipt>,
    receipt_reads: ReceiptReads,
    observed_targets: BTreeSet<String>,
    field_entry: FieldEntryGuard,
    targets: BTreeMap<String, Binding<Selector>>,
    follower_targets: BTreeSet<String>,
    target_roles: BTreeMap<(String, String), Binding<Selector>>,
    ability_holders: BTreeSet<String>,
    recipient_targets: BTreeMap<String, Binding<Vec<super::DamageRecipient>>>,
    parameters: Vec<super::Decision>,
    resolution_scope: ScopeId,
    resolution_captures: Vec<super::Capture>,
    event_captures: Vec<super::Capture>,
    event_trigger: Option<TriggerSpec>,
    formal_play: bool,
    auto: Option<&'definitions ast::Auto>,
    mode_targets: BTreeMap<u64, Vec<ast::Target>>,
}
impl<'definitions> BodyContext<'definitions> {
    fn new(
        entry: &'definitions ast::Ability,
        scope: ScopeId,
        body: &'definitions ast::Body,
        payment_result: Option<(String, PaymentReference)>,
        card_allocator: &'definitions mut Allocator,
    ) -> Self {
        Self {
            x: None,
            static_zone: entry
                .r#static
                .as_ref()
                .and_then(|head| head.zone.as_ref())
                .and_then(ast::ActivationZones::single),
            activation_zone: entry.activate.as_ref().and_then(|head| {
                head.zone
                    .as_ref()
                    .and_then(ast::ActivationZones::single)
                    .or_else(|| head.fusion.as_ref().map(|_| ZoneKind::Hand))
            }),
            subject_event: None,
            damage_event: None,
            dice_event: None,
            dice_results: BTreeMap::new(),
            equip_event: None,
            drive_checked_event: None,
            source_use: &entry.sources[0],
            counter_read_view: CounterReadView::Current,
            leave_event: None,
            discard_event: None,
            banish_event: None,
            hand_exit: None,
            discard_pp_event: None,
            entry_event: None,
            evolution: entry.evolve.as_ref(),
            source_can_enter_field: false,
            departure_to_cemetery: false,
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
            players: BTreeMap::new(),
            searches: BTreeSet::new(),
            ex_outputs: BTreeMap::new(),
            field_outputs: BTreeMap::new(),
            evolution_targets: BTreeSet::new(),
            ex_targets: BTreeSet::new(),
            flip_targets: BTreeSet::new(),
            hand_targets: BTreeSet::new(),
            playable_targets: BTreeSet::new(),
            opponent_cemetery_targets: BTreeSet::new(),
            cemetery_return_targets: BTreeSet::new(),
            nested_targets: nested_targets(&body.steps),
            observations: BTreeMap::new(),
            receipt_reads: ReceiptReads::default(),
            observed_targets: BTreeSet::new(),
            field_entry: FieldEntryGuard::default(),
            targets: BTreeMap::new(),
            follower_targets: BTreeSet::new(),
            target_roles: BTreeMap::new(),
            ability_holders: BTreeSet::new(),
            recipient_targets: BTreeMap::new(),
            parameters: vec![],
            resolution_scope: scope,
            resolution_captures: vec![],
            event_captures: vec![],
            event_trigger: None,
            mode_targets: BTreeMap::new(),
            auto: entry.auto.as_ref(),
            formal_play: entry.activate.is_some()
                || entry.spell.is_some()
                || body.optional_cost.is_some()
                || body.head_cost.is_some()
                || (entry.activate.is_none() && entry.spell.is_none() && !body.targets.is_empty()),
            local_payment: None,
        }
    }

    fn prepare_event(
        &mut self,
        entry: &ast::Ability,
        keyword: Option<KeywordId>,
        body: &ast::Body,
        path: &str,
    ) -> Result<Option<TriggerSpec>, LoweringError> {
        if entry.activate.is_some() || entry.spell.is_some() {
            return Ok(None);
        }
        let trigger = self.trigger(entry, keyword, body, path)?;
        self.discard_event = super::events::discard::self_event(&trigger);
        self.banish_event = super::events::banish::self_event(&trigger);
        if let TriggerCondition::Event { pattern } = &trigger.on {
            self.subject_event = SubjectFields::from_pattern(pattern);
            self.equip_event = super::events::equipment::binding(pattern);
            self.drive_checked_event = super::events::drive::checked_binding(pattern);
            if let EventPattern::Dice { binding, .. } = pattern {
                self.dice_event = Some(*binding);
            }
            if let EventPattern::Damage { binding, .. } = pattern {
                self.damage_event = Some(*binding);
            }
        }
        self.event_trigger = Some(trigger.clone());
        Ok(Some(trigger))
    }

    fn trigger(
        &mut self,
        entry: &ast::Ability,
        keyword: Option<KeywordId>,
        body: &ast::Body,
        path: &str,
    ) -> Result<TriggerSpec, LoweringError> {
        let event_id = match self
            .leave_event
            .map(Binding::key)
            .or_else(|| self.entry_event.map(Binding::key))
        {
            Some((_, id)) => id,
            None => self.allocator.id(path)?,
        };
        let subject_id = self.allocator.id(&format!("{path}/played-card"))?;
        let facet_id = self.allocator.id(path)?;
        let mut trigger = lowered_trigger(
            entry,
            keyword,
            EventIds {
                event: event_id,
                subject: subject_id,
                facet: facet_id,
                scope: self.scope,
            },
            path,
            &mut self.allocator,
        )?;
        if matches!(self.hand_exit, Some(hand_exit::Branch::Banish)) {
            trigger.on = TriggerCondition::Event {
                pattern: hand_exit::banish(event_id, self.scope),
            };
        }
        if body.per_turn == Some(0) {
            return Err(unsupported(path));
        }
        trigger.limit = body
            .per_turn
            .map(|maximum| super::LimitSpec::triggered_per_turn(Nat(maximum)));
        Ok(trigger)
    }

    fn prepare_parameters(
        &mut self,
        body: &ast::Body,
        path: &str,
        triggered: bool,
    ) -> Result<bool, LoweringError> {
        self.prepare_x(body, path)?;
        let reserved = self.allocator.next;
        for value in body.define.values() {
            // Set extrema retain empty-set semantics in their consuming selector.
            if matches!(value, ast::Number::Aggregate { agg }
                if matches!(agg.function, ast::AggregateFunction::Min | ast::AggregateFunction::Max))
            {
                continue;
            }
            self.number(value, path, &mut BTreeSet::new())?;
        }
        self.resolution_captures.clear();
        // Qualification candidates must stay distinct from later formal target bindings.
        self.allocator = Allocator {
            next: reserved,
            ..Allocator::default()
        };
        let trigger_choice = body.optional_cost.is_none()
            && body.head_cost.is_none()
            && play::trigger_choice(body, triggered, path)?;
        self.formal_play |= trigger_choice;
        let choice = if trigger_choice {
            true
        } else {
            play::choice_body(body, path)?
        };
        if self.x.is_some() && choice {
            return Err(unsupported(path));
        }
        self.targets(&body.targets, choice, path)?;
        Ok(trigger_choice)
    }

    fn body(
        &mut self,
        body: &ast::Body,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Program, LoweringError> {
        let previous_counter_view = self.counter_read_view;
        self.counter_read_view = CounterReadView::SourceInformation;
        let lowered = self.steps(&body.steps, &format!("{path}/do"), source, ability_source);
        self.counter_read_view = previous_counter_view;
        let mut program = lowered?;
        program.steps.splice(
            0..0,
            self.resolution_captures.drain(..).map(|capture| Located {
                source: source.clone(),
                node: Node::Capture { capture },
            }),
        );
        Ok(program)
    }

    fn located_step(source: &SourceSpan, path: &str, node: Node) -> Located<Node> {
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        Located {
            source: located,
            node,
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Step dispatch shares one binding and receipt environment."
    )]
    fn steps(
        &mut self,
        author_steps: &[ast::Step],
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Program, LoweringError> {
        let mut steps = Vec::new();
        let mut authored = author_steps.iter().enumerate();
        while let Some((index, step)) = authored.next() {
            let step_path = format!("{path}/{index}");
            self.check_paid_material_name(step, &step_path)?;
            if let ast::Step::Choose(choice) = step
                && matches!(choice.from, ast::ChooseFrom::Advance(_))
            {
                let Some((next, ast::Step::Move(movement))) = authored.next() else {
                    return Err(unsupported(&step_path));
                };
                steps.push(self.advance_entry(
                    choice,
                    movement,
                    &step_path,
                    &format!("{path}/{next}"),
                    source,
                    ability_source,
                )?);
                continue;
            }
            self.field_entry.adjacent = choice_before(author_steps, index);
            if let ast::Step::PlayAbility(value) = step {
                steps.extend(self.ability_play(value, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Counter(value) = step
                && !matches!(value.on, ast::Objects::This(_))
            {
                steps.push(self.counter_step(value, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::StackPlus(amount) = step {
                steps.push(self.stack_plus(amount, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Optional(optional) = step {
                steps.push(self.optional(optional, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Pay(payment) = step {
                steps.push(self.resolution_payment(payment, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Repeat(repeat) = step {
                let node = self.fixed_repeat(repeat, &step_path, source, ability_source)?;
                steps.push(Self::located_step(source, &step_path, node));
                continue;
            }
            if let ast::Step::EachPlayer(each) = step {
                let node = self.each_player(each, &step_path, source, ability_source)?;
                steps.push(Self::located_step(source, &step_path, node));
                continue;
            }
            if let ast::Step::Evolve(evolution) = step {
                steps.push(self.effect_evolution(evolution, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::If(conditional) = step {
                let condition = self.condition(&conditional.cond, &step_path)?;
                let parent = self.scope;
                self.scope = self.card_allocator.scope(&step_path)?;
                let previous_payment = self.discard_pp_event;
                self.discard_pp_event = self.discard_event.filter(|_| {
                    super::optional_payment::public_pp_guard(&condition)
                        && conditional.r#else.is_none()
                        && matches!(conditional.then.as_slice(), [ast::Step::Pay(_)])
                });
                let then_body = self.steps(
                    &conditional.then,
                    &format!("{step_path}/if/then"),
                    source,
                    ability_source,
                )?;
                self.discard_pp_event = previous_payment;
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
                steps.push(Self::located_step(
                    source,
                    &step_path,
                    Node::Control {
                        control: super::Control::If {
                            condition,
                            then_body,
                            else_body,
                        },
                    },
                ));
                continue;
            }
            // The ability entry admits one top-level group; mode bodies reject nested choices.
            if let ast::Step::Either(either) = step {
                steps.push(self.either(either, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Choice(choice) = step {
                steps.push(self.choice(choice, &step_path, source, ability_source)?);
                continue;
            }
            if let ast::Step::Choose(choose) = step {
                steps.push(self.choose(choose, &step_path, source)?);
                continue;
            }
            if let ast::Step::Move(movement) = step
                && Self::unordered_deck(movement)
            {
                if matches!(movement.order, Some(ast::MoveOrder::Shuffle)) {
                    steps.extend(self.shuffle_pair(
                        movement,
                        None,
                        &step_path,
                        source,
                        ability_source,
                    )?);
                } else {
                    let Some((next, following)) = authored.next() else {
                        return Err(unsupported(&step_path));
                    };
                    let next_path = format!("{path}/{next}");
                    steps.extend(self.shuffle_pair(
                        movement,
                        Some((following, &next_path)),
                        &step_path,
                        source,
                        ability_source,
                    )?);
                }
                continue;
            }
            if let ast::Step::Reveal(value) = step
                && value.top.is_some()
            {
                steps.extend(self.reveal_top(value, &step_path, source, ability_source)?);
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
                        actor: action_actor(&action),
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

    fn nested_card_play(&self, value: &ast::PlayCard, path: &str) -> Result<Action, LoweringError> {
        if !self.playable_targets.contains(&value.r#ref) || value.cost != Some(0) {
            return Err(unsupported(path));
        }
        let selection = *self
            .targets
            .get(&value.r#ref)
            .ok_or_else(|| unsupported(path))?;
        Ok(Action::PlayCard {
            card: ObjectOperand::Selected { selection },
            controller: PlayerRef::Controller {},
            cost_override: value
                .cost
                .map(|_| super::CostOverride::SetBase { amount: Int(0) }),
            authorization: super::EffectReference::CurrentInstallation {},
            parameters: super::PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![],
                costs: super::CostSpec::Ordered { steps: vec![] },
            },
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Every closed author step maps to its instruction in one exhaustive dispatch."
    )]
    fn action(
        &mut self,
        step: &ast::Step,
        path: &str,
        source: &AbilitySource,
        span: &SourceSpan,
    ) -> Result<Action, LoweringError> {
        match step {
            ast::Step::Dice(value) => self.dice(value, path),
            ast::Step::Win(_) => Ok(effect_victory(source)),
            ast::Step::PlayCard(value) => self.nested_card_play(value, path),
            ast::Step::ExtraTurn(_) => Ok(extra_turn(Binding::new(
                self.allocator.id(path)?,
                self.scope,
            ))),
            ast::Step::Counter(value) => counters::change(value, counters::holder(), path),
            ast::Step::GainDrive(_) => Ok(ride::gain_drive()),
            ast::Step::Race(value) => Ok(resource_entry::action(
                value.n.unwrap_or(1),
                super::MealRaceOperation::Race,
            )),
            ast::Step::StackPlus(_)
            | ast::Step::PlayAbility(_)
            | ast::Step::Choice(_)
            | ast::Step::Optional(_)
            | ast::Step::Pay(_)
            | ast::Step::Either(_)
            | ast::Step::Choose(_)
            | ast::Step::If(_)
            | ast::Step::EachPlayer(_)
            | ast::Step::Repeat(_)
            | ast::Step::Evolve(_) => Err(unsupported(path)),
            ast::Step::Destroy(value) => Ok(Action::Destroy {
                objects: match value {
                    ast::ObjectAction::Short(objects)
                    | ast::ObjectAction::Long { r#ref: objects, .. } => {
                        if matches!(objects, ast::Objects::Bound(name) if self.hand_targets.contains(name))
                        {
                            return Err(unsupported(path));
                        }
                        self.destroy_objects(objects, path)?
                    }
                },
            }),
            ast::Step::FlipDown(value) => self.flip_down(value, path),
            ast::Step::Banish(value) => self.banish(value, path),
            ast::Step::Transform(value) => self.transform(value, path),
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
            ast::Step::PlayerEffect(value) => self.player_effect(value, source, path),
            ast::Step::Search(value) => self.search(value, path),
            ast::Step::Reveal(value) => self.reveal(value, path),
            ast::Step::RevealUntil(value) => reveal_until::lower(self, value, path),
            ast::Step::Create(value) => self.create(value, path),
            ast::Step::Pilot(value) => numeric::pilot(value, path),
            ast::Step::Boxed(value) => self.boxed(value, path),
            ast::Step::Equip(value) => {
                let ast::This::This = value.on;
                if value.token.is_empty() {
                    return Err(unsupported(path));
                }
                Ok(Action::Equip {
                    holder: ObjectOperand::SourceHolder {},
                    template: super::TokenTemplateId(value.token.clone()),
                    count: Nat(1),
                })
            }
            ast::Step::Shuffle(value) => Ok(shuffle::full(value)),
            ast::Step::Look(value) => {
                if !matches!(
                    value.top.who,
                    ast::DeckHolder::Side(ast::LeaderSide::Controller)
                ) {
                    return Err(unsupported(path));
                }
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
            ast::Step::Draw(draw) => self.draw(draw, path),
            ast::Step::Damage(damage) => self.damage(damage, path),
            ast::Step::Life(life) => self.life(life, path),
            ast::Step::SetLife(life) => self.set_life(life, path),
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

fn effect_victory(source: &AbilitySource) -> Action {
    Action::GameResult {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        result: super::Outcome::Win,
        source: source.clone(),
    }
}

const fn extra_turn(occurrence: Binding<super::ScheduleToken>) -> Action {
    Action::ScheduleTurn {
        player: PlayerRef::Controller {},
        operation: super::TurnOperation::InsertBefore {
            anchor: super::ScheduleAnchor::NextTurn {},
        },
        occurrence,
    }
}

fn action_actor(action: &Action) -> PlayerRef {
    if let Action::Discard { player, .. } | Action::Draw { player, .. } = action {
        player.clone()
    } else {
        PlayerRef::Controller {}
    }
}
