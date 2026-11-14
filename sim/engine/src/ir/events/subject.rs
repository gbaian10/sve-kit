//! Public event subscriptions share one closed plan over the committed event subject.
use crate::ir::{AbilitiesRead, AbilityPredicate, KeywordId};
use crate::ir::{
    AttackEvent, Binding, Bool, BoolRead, BooleanState, CardKind, CardName, CharacteristicView,
    Characteristics, CharacteristicsFields, ClassFrozen, ClassId, ClassRead, Comparison,
    DamageEvent, DestroyEvent, EnterExEvent, EnterFieldEvent, EventContext, EventPattern,
    EventPayload, EventSnapshotPolicy, EvolveEvent, Expr, Info, Int, IntOperation, KindFrozen,
    KindRead, LeaveFieldEvent, NamesFrozen, NamesRead, NumberRead, ObjectFrozen, ObjectOperand,
    ObjectRef, ObjectView, PlayEvent, PlayerFrozen, PlayerId, PlayerRead, PlayerRef, PostureEvent,
    Predicate, Role, SnapshotFields, TraitId, TraitsFrozen, TraitsRead, UbActivatedEvent,
    ValueEquality, ZoneFrozen, ZoneKind, ZoneOperation,
};

#[derive(Clone, Copy)]
pub(crate) enum SubjectFields {
    Attack(Binding<AttackEvent>),
    Posture(Binding<PostureEvent>),
    DamageSource(Binding<DamageEvent>),
    Played {
        event: Binding<PlayEvent>,
        card: Binding<ObjectRef>,
    },
    Entry(Binding<EnterFieldEvent>),
    Destroy(Binding<DestroyEvent>),
    Evolve(Binding<EvolveEvent>),
    Cemetery(Binding<LeaveFieldEvent>),
    Hand(Binding<LeaveFieldEvent>),
    Leave(Binding<LeaveFieldEvent>),
    Ex(Binding<EnterExEvent>),
    Ub(Binding<UbActivatedEvent>),
}

pub(super) fn played_view(event: Binding<PlayEvent>, card: Binding<ObjectRef>) -> ObjectView {
    ObjectView::EventSnapshot {
        object: Box::new(ObjectOperand::FromExpression {
            value: Box::new(Expr::Bound(card)),
        }),
        event: EventContext::Play { event },
        policy: EventSnapshotPolicy::AtReveal,
    }
}

fn ub_view(event: Binding<UbActivatedEvent>) -> ObjectView {
    ObjectView::EventSnapshot {
        object: Box::new(ObjectOperand::FromExpression {
            value: Box::new(Expr::Frozen(Box::new(ObjectFrozen::UbActivatedHolder {
                event,
            }))),
        }),
        event: EventContext::UbActivated { event },
        policy: EventSnapshotPolicy::AtReveal,
    }
}

pub(super) fn attack_view(event: Binding<AttackEvent>) -> ObjectView {
    ObjectView::EventSnapshot {
        object: Box::new(ObjectOperand::FromExpression {
            value: Box::new(Expr::Frozen(Box::new(ObjectFrozen::AttackAttacker {
                event,
            }))),
        }),
        event: EventContext::Attack { event },
        policy: EventSnapshotPolicy::AfterEntry,
    }
}

impl SubjectFields {
    pub(crate) const fn from_pattern(pattern: &EventPattern) -> Option<Self> {
        if let EventPattern::UbActivated { binding, .. } = pattern {
            Some(Self::Ub(*binding))
        } else if let EventPattern::EnterField { binding, .. } = pattern {
            Some(Self::Entry(*binding))
        } else if let EventPattern::EnterEx { binding, .. } = pattern {
            Some(Self::Ex(*binding))
        } else if let EventPattern::Attack { binding, .. } = pattern {
            Some(Self::Attack(*binding))
        } else if let EventPattern::Evolve { binding, .. } = pattern {
            Some(Self::Evolve(*binding))
        } else if let EventPattern::Destroy { binding, .. } = pattern {
            Some(Self::Destroy(*binding))
        } else if let EventPattern::LeaveField { binding, .. } = pattern {
            Some(Self::Leave(*binding))
        } else {
            None
        }
    }

    pub(crate) fn payload_object(self, payload: &EventPayload) -> Option<ObjectRef> {
        Some(match (self, payload) {
            (Self::Ub(_), EventPayload::UbActivated { event }) => event.holder,
            (Self::Entry(_), EventPayload::EnterField { event }) => event.object,
            (Self::Ex(_), EventPayload::EnterEx { event }) => event.object,
            (Self::Attack(_), EventPayload::Attack { event }) => event.attacker,
            (Self::Evolve(_), EventPayload::Evolve { event }) => event.subject,
            (Self::Destroy(_), EventPayload::Destroy { event }) => event.object_before,
            (
                Self::Cemetery(_) | Self::Hand(_) | Self::Leave(_),
                EventPayload::LeaveField { event },
            ) => event.object_before,
            _ => return None,
        })
    }

    fn original_cost(self) -> Option<Expr<Int>> {
        let (event, policy) = match self {
            // A departure keeps the printed cost it had on the field, wherever it arrives.
            Self::Leave(event) | Self::Cemetery(event) | Self::Hand(event) => (
                EventContext::LeaveField { event },
                EventSnapshotPolicy::BeforeExit,
            ),
            Self::Entry(event) => (
                EventContext::EnterField { event },
                EventSnapshotPolicy::AfterEntry,
            ),
            Self::Evolve(event) => (
                EventContext::Evolve { event },
                EventSnapshotPolicy::AfterEntry,
            ),
            Self::Attack(_)
            | Self::Posture(_)
            | Self::DamageSource(_)
            | Self::Played { .. }
            | Self::Destroy(_)
            | Self::Ex(_)
            | Self::Ub(_) => return None,
        };
        Some(Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Cost {
                object: ObjectView::EventSnapshot {
                    object: Box::new(ObjectOperand::FromExpression {
                        value: Box::new(self.object()),
                    }),
                    event,
                    policy,
                },
                view: CharacteristicView::Original,
            },
        })))
    }

    fn name_contains(self, fragment: &CardName) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::NameContains {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::FromExpression {
                    value: Box::new(self.object()),
                }),
            },
            name: Expr::Literal(fragment.clone()),
        }))
    }

    fn known_class(self) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::Not {
            predicate: Expr::Compute(Box::new(Predicate::ClassEquals {
                left: self.class(),
                right: Expr::Literal(Info::AbsentByRules),
            })),
        }))
    }

    fn known_original_cost(self) -> Option<Expr<Bool>> {
        let Expr::Compute(operation) = self.original_cost()? else {
            return None;
        };
        let IntOperation::InformationOrZero { information } = *operation else {
            return None;
        };
        Some(Expr::Compute(Box::new(Predicate::Not {
            predicate: Expr::Compute(Box::new(Predicate::Equal {
                operands: Box::new(ValueEquality::InfoInt {
                    left: Expr::Live(Box::new(information)),
                    right: Expr::Literal(Info::AbsentByRules),
                }),
            })),
        })))
    }

    fn keyword(self, keyword: KeywordId) -> Option<Expr<Bool>> {
        let Self::Cemetery(event) = self else {
            return None;
        };
        Some(Expr::Compute(Box::new(Predicate::HasAbility {
            abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
                object: ObjectView::EventSnapshot {
                    object: Box::new(ObjectOperand::FromExpression {
                        value: Box::new(self.object()),
                    }),
                    event: EventContext::LeaveField { event },
                    policy: EventSnapshotPolicy::BeforeExit,
                },
                view: CharacteristicView::Effective,
            })),
            predicate: Box::new(AbilityPredicate {
                kind: None,
                keyword: Some(keyword),
                except: vec![],
                holder: None,
                origin: None,
            }),
        })))
    }

    fn parsed_keyword(self, expression: &Expr<Bool>) -> Option<KeywordId> {
        let Expr::Compute(operation) = expression else {
            return None;
        };
        let Predicate::HasAbility { predicate, .. } = operation.as_ref() else {
            return None;
        };
        let keyword = predicate.keyword?;
        (keyword == KeywordId::Ward && self.keyword(keyword).as_ref() == Some(expression))
            .then_some(keyword)
    }

    pub(crate) fn object(self) -> Expr<ObjectRef> {
        Expr::Frozen(Box::new(match self {
            Self::Attack(event) => ObjectFrozen::AttackAttacker { event },
            Self::Posture(event) => ObjectFrozen::PostureObject { event },
            Self::DamageSource(event) => ObjectFrozen::SnapshotObject {
                snapshot: Expr::Frozen(Box::new(SnapshotFields::DamageSource { event })),
            },
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                ObjectFrozen::LeaveFieldObjectBefore { event }
            }
            Self::Played { card, .. } => return Expr::Bound(card),
            Self::Entry(event) => ObjectFrozen::EnterFieldObject { event },
            Self::Destroy(event) => ObjectFrozen::DestroyObjectBefore { event },
            Self::Evolve(event) => ObjectFrozen::EvolveSubject { event },
            Self::Ex(event) => ObjectFrozen::EnterExObject { event },
            Self::Ub(event) => ObjectFrozen::UbActivatedHolder { event },
        }))
    }
    fn controller(self) -> Expr<PlayerRef> {
        match self {
            Self::Ub(event) => {
                Expr::Frozen(Box::new(PlayerFrozen::UbActivatedController { event }))
            }
            Self::Attack(event) => Expr::Frozen(Box::new(PlayerFrozen::AttackController { event })),
            Self::DamageSource(event) => {
                Expr::Frozen(Box::new(PlayerFrozen::DamageController { event }))
            }
            Self::Played { event, .. } => {
                Expr::Frozen(Box::new(PlayerFrozen::PlayController { event }))
            }
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                Expr::Frozen(Box::new(PlayerFrozen::LeaveFieldController { event }))
            }
            Self::Entry(event) => {
                Expr::Frozen(Box::new(PlayerFrozen::EnterFieldController { event }))
            }
            Self::Destroy(event) => {
                Expr::Frozen(Box::new(PlayerFrozen::DestroyController { event }))
            }
            Self::Ex(event) => Expr::Frozen(Box::new(PlayerFrozen::EnterExController { event })),
            // Evolution keeps its field generation; the bridge tests this role at commit.
            Self::Evolve(_) | Self::Posture(_) => Expr::Live(Box::new(PlayerRead::Role {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::FromExpression {
                        value: Box::new(self.object()),
                    }),
                },
                role: Role::Controller,
            })),
        }
    }
    fn kind(self) -> Expr<CardKind> {
        Expr::Frozen(Box::new(match self {
            Self::Ub(event) => {
                return Expr::Live(Box::new(KindRead::Kind {
                    object: ub_view(event),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Posture(_) => {
                return Expr::Live(Box::new(KindRead::Kind {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::FromExpression {
                            value: Box::new(self.object()),
                        }),
                    },
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Attack(event) => {
                return Expr::Live(Box::new(KindRead::Kind {
                    object: attack_view(event),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::DamageSource(event) => KindFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::DamageSource { event })),
            },
            Self::Played { event, card } => {
                return Expr::Live(Box::new(KindRead::Kind {
                    object: played_view(event, card),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Entry(event) => KindFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterFieldCharacters {
                    event,
                })),
            },
            Self::Destroy(event) => KindFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::DestroyCharacters {
                    event,
                })),
            },
            Self::Evolve(event) => KindFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EvolveAfter { event })),
            },
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                KindFrozen::Snapshot {
                    object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
                }
            }
            Self::Ex(event) => KindFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
        }))
    }
    fn traits(self) -> Expr<Vec<TraitId>> {
        Expr::Frozen(Box::new(match self {
            Self::Ub(event) => {
                return Expr::Live(Box::new(TraitsRead::Traits {
                    object: ub_view(event),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Posture(_) => {
                return Expr::Live(Box::new(TraitsRead::Traits {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::FromExpression {
                            value: Box::new(self.object()),
                        }),
                    },
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Attack(event) => {
                return Expr::Live(Box::new(TraitsRead::Traits {
                    object: attack_view(event),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::DamageSource(event) => TraitsFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::DamageSource { event })),
            },
            Self::Played { event, card } => {
                return Expr::Live(Box::new(TraitsRead::Traits {
                    object: played_view(event, card),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Entry(event) => TraitsFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterFieldCharacters {
                    event,
                })),
            },
            Self::Destroy(event) => TraitsFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::DestroyCharacters {
                    event,
                })),
            },
            Self::Evolve(event) => TraitsFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EvolveAfter { event })),
            },
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                TraitsFrozen::Snapshot {
                    object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
                }
            }
            Self::Ex(event) => TraitsFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
        }))
    }
    fn names(self) -> Expr<Vec<CardName>> {
        Expr::Frozen(Box::new(match self {
            Self::Ub(event) => {
                return Expr::Live(Box::new(NamesRead::Names {
                    object: ub_view(event),
                }));
            }
            Self::Posture(_) => {
                return Expr::Live(Box::new(NamesRead::Names {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::FromExpression {
                            value: Box::new(self.object()),
                        }),
                    },
                }));
            }
            Self::Attack(event) => {
                return Expr::Live(Box::new(NamesRead::Names {
                    object: attack_view(event),
                }));
            }
            Self::DamageSource(event) => NamesFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::DamageSource { event })),
            },
            Self::Played { event, card } => {
                return Expr::Live(Box::new(NamesRead::Names {
                    object: played_view(event, card),
                }));
            }
            Self::Entry(event) => NamesFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterFieldCharacters {
                    event,
                })),
            },
            Self::Destroy(event) => NamesFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::DestroyCharacters {
                    event,
                })),
            },
            Self::Evolve(event) => NamesFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EvolveAfter { event })),
            },
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                NamesFrozen::Snapshot {
                    object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
                }
            }
            Self::Ex(event) => NamesFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
        }))
    }
    fn class(self) -> Expr<Info<ClassId>> {
        let field = match self {
            Self::Ub(event) => {
                return Expr::Live(Box::new(ClassRead::Class {
                    object: ub_view(event),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Posture(_) => {
                return Expr::Live(Box::new(ClassRead::Class {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::FromExpression {
                            value: Box::new(self.object()),
                        }),
                    },
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Attack(event) => {
                return Expr::Live(Box::new(ClassRead::Class {
                    object: attack_view(event),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::DamageSource(event) => ClassFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::DamageSource { event })),
            },
            Self::Played { event, card } => {
                return Expr::Live(Box::new(ClassRead::Class {
                    object: played_view(event, card),
                    view: CharacteristicView::Effective,
                }));
            }
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                ClassFrozen::Snapshot {
                    object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
                }
            }
            Self::Entry(event) => ClassFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterFieldCharacters {
                    event,
                })),
            },
            Self::Destroy(event) => ClassFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::DestroyCharacters {
                    event,
                })),
            },
            Self::Ex(event) => ClassFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
            Self::Evolve(event) => ClassFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EvolveAfter { event })),
            },
        };
        Expr::Frozen(Box::new(field))
    }
    fn token(self) -> Option<Expr<Bool>> {
        let object = match self {
            Self::Ub(event) => ub_view(event),
            Self::Attack(event) => attack_view(event),
            Self::DamageSource(event) => ObjectView::Snapshot {
                value: Box::new(Expr::Frozen(Box::new(SnapshotFields::DamageSource {
                    event,
                }))),
            },
            Self::Played { event, card } => played_view(event, card),
            Self::Cemetery(event) | Self::Hand(event) | Self::Leave(event) => {
                ObjectView::Snapshot {
                    value: Box::new(Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki {
                        event,
                    }))),
                }
            }
            Self::Entry(_) | Self::Posture(_) => ObjectView::Current {
                object: Box::new(ObjectOperand::FromExpression {
                    value: Box::new(self.object()),
                }),
            },
            Self::Destroy(_) | Self::Evolve(_) | Self::Ex(_) => return None,
        };
        Some(Expr::Live(Box::new(BoolRead::State {
            object,
            property: BooleanState::TokenStatus,
        })))
    }
    pub(crate) const fn view(self) -> EventSnapshotPolicy {
        match self {
            Self::Played { .. } | Self::Ub(_) => EventSnapshotPolicy::AtReveal,
            Self::Cemetery(_) | Self::Hand(_) | Self::Leave(_) | Self::Destroy(_) => {
                EventSnapshotPolicy::BeforeExit
            }
            Self::Posture(_)
            | Self::Attack(_)
            | Self::DamageSource(_)
            | Self::Entry(_)
            | Self::Evolve(_)
            | Self::Ex(_) => EventSnapshotPolicy::AfterEntry,
        }
    }
    fn destination(self) -> Option<Expr<Bool>> {
        let (event, kind) = match self {
            Self::Cemetery(event) => (event, ZoneKind::Cemetery),
            Self::Hand(event) => (event, ZoneKind::Hand),
            Self::Ub(_)
            | Self::Attack(_)
            | Self::Posture(_)
            | Self::DamageSource(_)
            | Self::Played { .. }
            | Self::Entry(_)
            | Self::Destroy(_)
            | Self::Evolve(_)
            | Self::Leave(_)
            | Self::Ex(_) => return None,
        };
        Some(Expr::Compute(Box::new(Predicate::ZoneEquals {
            left: Expr::Frozen(Box::new(ZoneFrozen::LeaveFieldTo { event })),
            right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                player: PlayerRef::FromExpression {
                    value: Box::new(Expr::Live(Box::new(PlayerRead::Role {
                        object: ObjectView::Snapshot {
                            value: Box::new(Expr::Frozen(Box::new(
                                SnapshotFields::LeaveFieldLki { event },
                            ))),
                        },
                        role: Role::Owner,
                    }))),
                },
                kind,
            })),
        })))
    }
}

pub(crate) struct SubjectInformation<'subject> {
    pub(crate) original_cost: Option<i64>,
    pub(crate) keywords: &'subject [KeywordId],
    pub(crate) kind: Option<CardKind>,
    pub(crate) traits: &'subject [TraitId],
    pub(crate) names: &'subject [CardName],
    pub(crate) class: Option<&'subject ClassId>,
}

impl<'subject> SubjectInformation<'subject> {
    pub(crate) fn from_characters(
        characters: &'subject Characteristics,
        original_cost: Option<i64>,
    ) -> Self {
        Self {
            original_cost,
            keywords: &[],
            kind: Some(characters.kind),
            traits: &characters.traits,
            names: &characters.names,
            class: characters.class.as_ref(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubjectFilter {
    pub(crate) original_cost: Option<(Comparison, i64)>,
    pub(crate) keyword: Option<KeywordId>,
    pub(crate) own: Option<bool>,
    pub(crate) itself: Option<bool>,
    pub(crate) kind: Option<CardKind>,
    pub(crate) trait_id: Option<TraitId>,
    /// 「A・フォロワーかB・フォロワーが出たとき」 on an entering subject (BP21-008).
    pub(crate) traits_any: Vec<TraitId>,
    pub(crate) name: Option<CardName>,
    /// 「カード名に『…』を含む」 on an entering subject (BP13-027).
    pub(crate) name_contains: Option<CardName>,
    pub(crate) name_contains_any: Vec<CardName>,
    pub(crate) class: Option<ClassId>,
    pub(crate) not_class: Option<ClassId>,
    pub(crate) own_turn: bool,
    pub(crate) token: Option<bool>,
    /// 「これと同名を除く」 on an entering subject (BP22-003): compared with the observer's names.
    pub(crate) not_same_name: bool,
}

/// The entering subject shares no name with the observing card as it currently is.
fn holder_name_exclusion(fields: SubjectFields) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::Not {
        predicate: Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: fields.names(),
            right: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
            })),
        })),
    }))
}

pub(crate) fn has_keyword_filter(expression: &Expr<Bool>) -> bool {
    let Expr::Compute(predicate) = expression else {
        return false;
    };
    if matches!(predicate.as_ref(), Predicate::HasAbility { .. }) {
        return true;
    }
    if let Predicate::And { predicates } | Predicate::Or { predicates } = predicate.as_ref() {
        return predicates.iter().any(has_keyword_filter);
    }
    if let Predicate::Not { predicate } = predicate.as_ref() {
        return has_keyword_filter(predicate);
    }
    false
}

impl SubjectFilter {
    pub(crate) fn predicate(&self, fields: SubjectFields) -> Expr<Bool> {
        let mut predicates = fields.destination().into_iter().collect::<Vec<_>>();
        if let Some(keyword) = self.keyword {
            let Some(predicate) = fields.keyword(keyword) else {
                return Expr::Literal(Bool(false));
            };
            predicates.push(predicate);
        }
        if let Some(positive) = self.itself {
            let predicate = Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: fields.object(),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            }));
            predicates.push(if positive {
                predicate
            } else {
                Expr::Compute(Box::new(Predicate::Not { predicate }))
            });
        }
        if let Some(own) = self.own {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: fields.controller(),
                right: Expr::Literal(if own {
                    PlayerRef::Controller {}
                } else {
                    super::timing::opponent()
                }),
            })));
        }
        if let Some(kind) = self.kind {
            predicates.push(Expr::Compute(Box::new(Predicate::KindEquals {
                left: fields.kind(),
                right: Expr::Literal(kind),
            })));
        }
        if let Some(trait_id) = &self.trait_id {
            predicates.push(Expr::Compute(Box::new(Predicate::HasTrait {
                traits: fields.traits(),
                trait_id: trait_id.clone(),
            })));
        }
        if !self.traits_any.is_empty() {
            predicates.push(Expr::Compute(Box::new(Predicate::Or {
                predicates: self
                    .traits_any
                    .iter()
                    .map(|trait_id| {
                        Expr::Compute(Box::new(Predicate::HasTrait {
                            traits: fields.traits(),
                            trait_id: trait_id.clone(),
                        }))
                    })
                    .collect(),
            })));
        }
        if let Some(name) = &self.name {
            predicates.push(Expr::Compute(Box::new(Predicate::NamesIntersect {
                left: fields.names(),
                right: Expr::Literal(vec![name.clone()]),
            })));
        }
        if let Some(fragment) = &self.name_contains {
            predicates.push(fields.name_contains(fragment));
        }
        if self.not_same_name {
            predicates.push(holder_name_exclusion(fields));
        }
        if !self.name_contains_any.is_empty() {
            predicates.push(Expr::Compute(Box::new(Predicate::Or {
                predicates: self
                    .name_contains_any
                    .iter()
                    .map(|fragment| fields.name_contains(fragment))
                    .collect(),
            })));
        }
        self.append_class_predicates(fields, &mut predicates);
        if let (Some(positive), Some(predicate)) = (self.token, fields.token()) {
            predicates.push(if positive {
                predicate
            } else {
                Expr::Compute(Box::new(Predicate::Not { predicate }))
            });
        }
        if self.own_turn {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Active {}),
                right: Expr::Literal(PlayerRef::Controller {}),
            })));
        }
        if !self.append_cost_predicates(fields, &mut predicates) {
            return Expr::Literal(Bool(false));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }

    fn append_class_predicates(&self, fields: SubjectFields, predicates: &mut Vec<Expr<Bool>>) {
        if let Some(class) = &self.class {
            predicates.push(Expr::Compute(Box::new(Predicate::ClassEquals {
                left: fields.class(),
                right: Expr::Literal(Info::Present(class.clone())),
            })));
        }
        if let Some(class) = &self.not_class {
            predicates.push(fields.known_class());
            predicates.push(Expr::Compute(Box::new(Predicate::Not {
                predicate: Expr::Compute(Box::new(Predicate::ClassEquals {
                    left: fields.class(),
                    right: Expr::Literal(Info::Present(class.clone())),
                })),
            })));
        }
    }

    fn append_cost_predicates(
        &self,
        fields: SubjectFields,
        predicates: &mut Vec<Expr<Bool>>,
    ) -> bool {
        if let Some((comparison, cost)) = self.original_cost {
            let Some(left) = fields.original_cost() else {
                return false;
            };
            if comparison != Comparison::Equal || cost == 0 {
                let Some(known) = fields.known_original_cost() else {
                    return false;
                };
                predicates.push(known);
            }
            predicates.push(Expr::Compute(Box::new(Predicate::CompareInt {
                left,
                comparison,
                right: Expr::Literal(Int(cost)),
            })));
        }
        true
    }

    pub(crate) fn parse(fields: SubjectFields, expression: &Expr<Bool>) -> Option<Self> {
        let itself = Expr::Compute(Box::new(Predicate::ObjectEquals {
            left: fields.object(),
            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
        }));
        if matches!(fields, SubjectFields::Entry(_)) && *expression == itself {
            return Some(Self {
                itself: Some(true),
                ..Self::default()
            });
        }
        let Expr::Compute(root) = expression else {
            return None;
        };
        let Predicate::And { predicates } = root.as_ref() else {
            return None;
        };
        let mut plan = Self::default();
        for part in predicates {
            if fields.destination().as_ref() == Some(part) {
                continue;
            }
            if fields.token().as_ref() == Some(part) {
                plan.token = Some(true);
                continue;
            }
            if fields.token().is_some_and(|token| {
                *part == Expr::Compute(Box::new(Predicate::Not { predicate: token }))
            }) {
                plan.token = Some(false);
                continue;
            }
            let Expr::Compute(predicate) = part else {
                return None;
            };
            if *part == itself {
                plan.itself = Some(true);
            } else if matches!(predicate.as_ref(), Predicate::Not { predicate: inner } if *inner == itself)
            {
                plan.itself = Some(false);
            } else if matches!(predicate.as_ref(), Predicate::PlayerEquals { left, right } if *left == fields.controller() && *right == Expr::Literal(PlayerRef::Controller {}))
            {
                plan.own = Some(true);
            } else if matches!(predicate.as_ref(), Predicate::PlayerEquals { left, right } if *left == fields.controller() && *right == Expr::Literal(super::timing::opponent()))
            {
                plan.own = Some(false);
            } else if matches!(predicate.as_ref(), Predicate::PlayerEquals { left, right } if *left == Expr::Literal(PlayerRef::Active {}) && *right == Expr::Literal(PlayerRef::Controller {}))
            {
                plan.own_turn = true;
            } else {
                plan.characteristic(fields, part)?;
            }
        }
        (plan.closed_characteristics() && plan.predicate(fields) == *expression).then_some(plan)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Each closed characteristic predicate shape is recognised in one place."
    )]
    fn characteristic(&mut self, fields: SubjectFields, part: &Expr<Bool>) -> Option<()> {
        let Expr::Compute(predicate) = part else {
            return None;
        };
        if (matches!(fields, SubjectFields::Entry(_)) && *part == fields.known_class())
            || fields.known_original_cost().as_ref() == Some(part)
        {
            return Some(());
        }
        // Entering subjects and saved departure facts both carry names to compare (BP10-009).
        if matches!(
            fields,
            SubjectFields::Entry(_)
                | SubjectFields::Leave(_)
                | SubjectFields::Cemetery(_)
                | SubjectFields::Hand(_)
        ) && *part == holder_name_exclusion(fields)
        {
            self.not_same_name = true;
            return Some(());
        }
        if let Predicate::Not {
            predicate: Expr::Compute(inner),
        } = predicate.as_ref()
            && let Predicate::ClassEquals {
                left,
                right: Expr::Literal(Info::Present(class)),
            } = inner.as_ref()
        {
            if !matches!(fields, SubjectFields::Entry(_))
                || *left != fields.class()
                || class.0.is_empty()
            {
                return None;
            }
            self.not_class = Some(class.clone());
            return Some(());
        }
        if matches!(predicate.as_ref(), Predicate::HasAbility { .. }) {
            self.keyword = Some(fields.parsed_keyword(part)?);
        } else if let Predicate::CompareInt {
            left,
            comparison,
            right: Expr::Literal(Int(cost)),
        } = predicate.as_ref()
        {
            if *cost < 0
                || !matches!(
                    comparison,
                    Comparison::Equal | Comparison::LessEqual | Comparison::GreaterEqual
                )
                || fields.original_cost().as_ref() != Some(left)
            {
                return None;
            }
            self.original_cost = Some((*comparison, *cost));
        } else if let Predicate::KindEquals {
            left,
            right: Expr::Literal(kind),
        } = predicate.as_ref()
        {
            if *left != fields.kind() {
                return None;
            }
            self.kind = Some(*kind);
        } else if let Predicate::HasTrait { traits, trait_id } = predicate.as_ref() {
            if *traits != fields.traits() {
                return None;
            }
            self.trait_id = Some(trait_id.clone());
        } else if let Predicate::Or { predicates } = predicate.as_ref() {
            if self.trait_union(fields, predicates).is_none() {
                self.name_union(fields, predicates)?;
            }
        } else if let Predicate::ClassEquals {
            left,
            right: Expr::Literal(Info::Present(class)),
        } = predicate.as_ref()
        {
            if *left != fields.class() {
                return None;
            }
            self.class = Some(class.clone());
        } else if let Predicate::NamesIntersect {
            left,
            right: Expr::Literal(names),
        } = predicate.as_ref()
        {
            if *left != fields.names() || names.len() != 1 {
                return None;
            }
            self.name = names.first().cloned();
        } else if let Predicate::NameContains {
            object,
            name: Expr::Literal(fragment),
        } = predicate.as_ref()
        {
            let current = ObjectView::Current {
                object: Box::new(ObjectOperand::FromExpression {
                    value: Box::new(fields.object()),
                }),
            };
            if !matches!(fields, SubjectFields::Entry(_))
                || *object != current
                || fragment.0.is_empty()
            {
                return None;
            }
            self.name_contains = Some(fragment.clone());
        } else {
            return None;
        }
        Some(())
    }

    /// Only an entering subject reads a union of at least two distinct traits.
    fn trait_union(&mut self, fields: SubjectFields, branches: &[Expr<Bool>]) -> Option<()> {
        if !matches!(fields, SubjectFields::Entry(_)) || !self.traits_any.is_empty() {
            return None;
        }
        let mut traits = Vec::new();
        for branch in branches {
            let Expr::Compute(predicate) = branch else {
                return None;
            };
            let Predicate::HasTrait {
                traits: read,
                trait_id,
            } = predicate.as_ref()
            else {
                return None;
            };
            if *read != fields.traits() || trait_id.0.is_empty() || traits.contains(trait_id) {
                return None;
            }
            traits.push(trait_id.clone());
        }
        if traits.len() < 2 {
            return None;
        }
        self.traits_any = traits;
        Some(())
    }

    fn name_union(&mut self, fields: SubjectFields, branches: &[Expr<Bool>]) -> Option<()> {
        if !matches!(fields, SubjectFields::Entry(_)) || !self.name_contains_any.is_empty() {
            return None;
        }
        let mut fragments = Vec::new();
        for branch in branches {
            let Expr::Compute(predicate) = branch else {
                return None;
            };
            let Predicate::NameContains {
                name: Expr::Literal(fragment),
                ..
            } = predicate.as_ref()
            else {
                return None;
            };
            if fragment.0.is_empty()
                || fragments.contains(fragment)
                || *branch != fields.name_contains(fragment)
            {
                return None;
            }
            fragments.push(fragment.clone());
        }
        if fragments.len() < 2 {
            return None;
        }
        self.name_contains_any = fragments;
        Some(())
    }

    fn closed_characteristics(&self) -> bool {
        if !self.name_contains_any.is_empty() && self.name_contains.is_some() {
            return false;
        }
        if self.keyword.is_some() {
            return *self
                == (Self {
                    own: Some(true),
                    kind: Some(CardKind::Follower),
                    keyword: Some(KeywordId::Ward),
                    ..Self::default()
                });
        }
        true
    }

    pub(crate) fn matches_information(
        &self,
        characters: &SubjectInformation<'_>,
        same: bool,
        observer_controller: PlayerId,
        subject_controller: PlayerId,
        active: PlayerId,
        token: bool,
    ) -> bool {
        self.original_cost.is_none_or(|(comparison, expected)| {
            characters
                .original_cost
                .is_some_and(|cost| match comparison {
                    Comparison::Equal => cost == expected,
                    Comparison::LessEqual => cost <= expected,
                    Comparison::GreaterEqual => cost >= expected,
                    Comparison::NotEqual | Comparison::Less | Comparison::Greater => false,
                })
        }) && self
            .keyword
            .is_none_or(|keyword| characters.keywords.contains(&keyword))
            && self.token.is_none_or(|expected| expected == token)
            && self.itself.is_none_or(|itself| itself == same)
            && self
                .own
                .is_none_or(|own| own == (observer_controller == subject_controller))
            && self.kind.is_none_or(|kind| characters.kind == Some(kind))
            && self
                .trait_id
                .as_ref()
                .is_none_or(|id| characters.traits.contains(id))
            && (self.traits_any.is_empty()
                || self
                    .traits_any
                    .iter()
                    .any(|id| characters.traits.contains(id)))
            && self
                .name
                .as_ref()
                .is_none_or(|name| characters.names.contains(name))
            && self.name_contains.as_ref().is_none_or(|fragment| {
                characters
                    .names
                    .iter()
                    .any(|name| name.0.contains(&fragment.0))
            })
            && (self.name_contains_any.is_empty()
                || self.name_contains_any.iter().any(|fragment| {
                    characters
                        .names
                        .iter()
                        .any(|name| name.0.contains(&fragment.0))
                }))
            && self
                .class
                .as_ref()
                .is_none_or(|class| characters.class == Some(class))
            && self
                .not_class
                .as_ref()
                .is_none_or(|excluded| characters.class.is_some_and(|class| class != excluded))
            && (!self.own_turn || active == observer_controller)
    }
}
