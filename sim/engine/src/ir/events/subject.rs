//! Public event subscriptions share one closed plan over the committed event subject.
use crate::ir::{
    Binding, Bool, BoolRead, BooleanState, CardKind, CardName, Characteristics,
    CharacteristicsFields, ClassFrozen, ClassId, DestroyEvent, EnterExEvent, EnterFieldEvent,
    EventSnapshotPolicy, EvolveEvent, Expr, Info, KindFrozen, LeaveFieldEvent, NamesFrozen,
    ObjectFrozen, ObjectOperand, ObjectRef, ObjectView, PlayerFrozen, PlayerId, PlayerRead,
    PlayerRef, Predicate, Role, SnapshotFields, TraitId, TraitsFrozen, ZoneFrozen, ZoneKind,
    ZoneOperation,
};

#[derive(Clone, Copy)]
pub(crate) enum SubjectFields {
    Entry(Binding<EnterFieldEvent>),
    Destroy(Binding<DestroyEvent>),
    Evolve(Binding<EvolveEvent>),
    Cemetery(Binding<LeaveFieldEvent>),
    Leave(Binding<LeaveFieldEvent>),
    Ex(Binding<EnterExEvent>),
}

impl SubjectFields {
    fn object(self) -> Expr<ObjectRef> {
        Expr::Frozen(Box::new(match self {
            Self::Cemetery(event) | Self::Leave(event) => {
                ObjectFrozen::LeaveFieldObjectBefore { event }
            }
            Self::Entry(event) => ObjectFrozen::EnterFieldObject { event },
            Self::Destroy(event) => ObjectFrozen::DestroyObjectBefore { event },
            Self::Evolve(event) => ObjectFrozen::EvolveSubject { event },
            Self::Ex(event) => ObjectFrozen::EnterExObject { event },
        }))
    }
    fn controller(self) -> Expr<PlayerRef> {
        match self {
            Self::Cemetery(event) | Self::Leave(event) => {
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
            Self::Evolve(_) => Expr::Live(Box::new(PlayerRead::Role {
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
            Self::Cemetery(event) | Self::Leave(event) => KindFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
            },
            Self::Ex(event) => KindFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
        }))
    }
    fn traits(self) -> Expr<Vec<TraitId>> {
        Expr::Frozen(Box::new(match self {
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
            Self::Cemetery(event) | Self::Leave(event) => TraitsFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
            },
            Self::Ex(event) => TraitsFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
        }))
    }
    fn names(self) -> Expr<Vec<CardName>> {
        Expr::Frozen(Box::new(match self {
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
            Self::Cemetery(event) | Self::Leave(event) => NamesFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
            },
            Self::Ex(event) => NamesFrozen::Characters {
                characters: Expr::Frozen(Box::new(CharacteristicsFields::EnterExCharacters {
                    event,
                })),
            },
        }))
    }
    fn class(self) -> Expr<Info<ClassId>> {
        let field = match self {
            Self::Cemetery(event) | Self::Leave(event) => ClassFrozen::Snapshot {
                object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki { event })),
            },
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
            Self::Cemetery(event) | Self::Leave(event) => ObjectView::Snapshot {
                value: Box::new(Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki {
                    event,
                }))),
            },
            Self::Entry(_) => ObjectView::Current {
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
            Self::Cemetery(_) | Self::Leave(_) | Self::Destroy(_) => {
                EventSnapshotPolicy::BeforeExit
            }
            Self::Entry(_) | Self::Evolve(_) | Self::Ex(_) => EventSnapshotPolicy::AfterEntry,
        }
    }
    fn destination(self) -> Option<Expr<Bool>> {
        let Self::Cemetery(event) = self else {
            return None;
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
                kind: ZoneKind::Cemetery,
            })),
        })))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubjectFilter {
    pub(crate) own: Option<bool>,
    pub(crate) itself: Option<bool>,
    pub(crate) kind: Option<CardKind>,
    pub(crate) trait_id: Option<TraitId>,
    pub(crate) name: Option<CardName>,
    pub(crate) class: Option<ClassId>,
    pub(crate) own_turn: bool,
    pub(crate) token: Option<bool>,
}

impl SubjectFilter {
    pub(crate) fn predicate(&self, fields: SubjectFields) -> Expr<Bool> {
        let mut predicates = fields.destination().into_iter().collect::<Vec<_>>();
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
        if let Some(name) = &self.name {
            predicates.push(Expr::Compute(Box::new(Predicate::NamesIntersect {
                left: fields.names(),
                right: Expr::Literal(vec![name.clone()]),
            })));
        }
        if let Some(class) = &self.class {
            predicates.push(Expr::Compute(Box::new(Predicate::ClassEquals {
                left: fields.class(),
                right: Expr::Literal(Info::Present(class.clone())),
            })));
        }
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
        Expr::Compute(Box::new(Predicate::And { predicates }))
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
            } else if let Predicate::KindEquals {
                left,
                right: Expr::Literal(kind),
            } = predicate.as_ref()
            {
                if *left != fields.kind() {
                    return None;
                }
                plan.kind = Some(*kind);
            } else if let Predicate::HasTrait { traits, trait_id } = predicate.as_ref() {
                if *traits != fields.traits() {
                    return None;
                }
                plan.trait_id = Some(trait_id.clone());
            } else if let Predicate::ClassEquals {
                left,
                right: Expr::Literal(Info::Present(class)),
            } = predicate.as_ref()
            {
                if *left != fields.class() {
                    return None;
                }
                plan.class = Some(class.clone());
            } else if let Predicate::NamesIntersect {
                left,
                right: Expr::Literal(names),
            } = predicate.as_ref()
            {
                if *left != fields.names() || names.len() != 1 {
                    return None;
                }
                plan.name = names.first().cloned();
            } else {
                return None;
            }
        }
        (plan.predicate(fields) == *expression).then_some(plan)
    }

    pub(crate) fn matches(
        &self,
        characters: &Characteristics,
        same: bool,
        observer_controller: PlayerId,
        subject_controller: PlayerId,
        active: PlayerId,
        token: bool,
    ) -> bool {
        self.token.is_none_or(|expected| expected == token)
            && self.itself.is_none_or(|itself| itself == same)
            && self
                .own
                .is_none_or(|own| own == (observer_controller == subject_controller))
            && self.kind.is_none_or(|kind| kind == characters.kind)
            && self
                .trait_id
                .as_ref()
                .is_none_or(|id| characters.traits.contains(id))
            && self
                .name
                .as_ref()
                .is_none_or(|name| characters.names.contains(name))
            && self
                .class
                .as_ref()
                .is_none_or(|class| characters.class.as_ref() == Some(class))
            && (!self.own_turn || active == observer_controller)
    }
}
