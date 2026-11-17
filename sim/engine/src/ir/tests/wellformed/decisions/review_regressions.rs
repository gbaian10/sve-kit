use super::*;

fn opponents(kind: ZoneKind) -> Selector {
    let mut selector = zone(PlayerRef::Controller {}, kind, 10);
    if let Selector::Zone { players, .. } = &mut selector {
        players.relation = PlayerRelation::Opponents;
    }
    selector
}
fn bound_object_domain(element: Binding<ObjectRef>) -> Decision {
    chosen_objects(
        1,
        3,
        Selector::Objects {
            objects: vec![ObjectOperand::Bound { binding: element }],
        },
    )
}
fn state(binding: Binding<ObjectRef>) -> Expr<Bool> {
    Expr::Live(Box::new(BoolRead::State {
        object: ObjectView::Current {
            object: Box::new(ObjectOperand::Bound { binding }),
        },
        property: BooleanState::FaceUp,
    }))
}

#[test]
fn external_object_bindings_cannot_launder_hidden_candidates_or_attributes() {
    for input in [
        zone(PlayerRef::Controller {}, ZoneKind::Deck, 10),
        opponents(ZoneKind::Hand),
        hand(),
    ] {
        let element = Binding::new(2, ScopeId(3));
        let mut filtered = zone(PlayerRef::Controller {}, ZoneKind::Field, 11);
        if let Selector::Zone { predicate, .. } = &mut filtered {
            **predicate = state(element);
        }
        for selection in [bound_object_domain(element), chosen_objects(1, 3, filtered)] {
            reject(
                body(
                    1,
                    vec![control(Control::EachObject {
                        input: input.clone(),
                        binding: element,
                        body: body(3, vec![decision(selection)]),
                        order: None,
                    })],
                ),
                WellFormedReason::UnsupportedShape,
            );
        }
    }
}

#[test]
fn evolve_requires_visibility_evidence_and_banish_is_public() {
    let evolve_zone = ZoneKind::Evolve;
    for selector in [
        zone(PlayerRef::Controller {}, evolve_zone, 10),
        opponents(evolve_zone),
    ] {
        for visibility in [
            Visibility::Public {},
            Visibility::Only {
                players: controller_set(),
            },
        ] {
            let mut choice = chosen_objects(1, 1, selector.clone());
            choice.visibility = visibility;
            reject(
                body(1, vec![decision(choice)]),
                WellFormedReason::UnsupportedShape,
            );
        }
    }
    for kind in [
        ZoneKind::Field,
        ZoneKind::Ex,
        ZoneKind::Cemetery,
        ZoneKind::Banish,
        ZoneKind::Resolution,
        ZoneKind::Race,
        ZoneKind::Drive,
        ZoneKind::Trigger,
        ZoneKind::Equipment,
    ] {
        accept(body(
            1,
            vec![decision(chosen_objects(
                1,
                1,
                zone(PlayerRef::Controller {}, kind, 10),
            ))],
        ));
    }
}

#[test]
fn nonchoice_stages_allow_required_deterministic_costs_but_not_optional_payment() {
    let mut step = cost(DiscardMode::DownTo { count: Nat(2) }, None);
    step.instruction.action = Action::Posture {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        posture: Posture::Acted,
    };
    let optional = pay(vec![step], body(4, vec![]));
    reject(
        body(
            1,
            vec![each_player(
                optional.clone(),
                StageMode::NonChoiceSimultaneous {},
            )],
        ),
        WellFormedReason::UnsupportedShape,
    );
    let mut required = optional;
    if let Node::Control {
        control: Control::Pay {
            optional: optionality,
            ..
        },
    } = &mut required.node
    {
        *optionality = PaymentOptionality::Required;
    }
    accept(body(
        1,
        vec![each_player(required, StageMode::NonChoiceSimultaneous {})],
    ));
}

fn answer_dependent_choice() -> Decision {
    let mut selector = zone(PlayerRef::Controller {}, ZoneKind::Field, 10);
    if let Selector::Zone { predicate, .. } = &mut selector {
        **predicate = positive(Expr::Bound(Binding::new(1, ScopeId(1))));
    }
    chosen_objects(2, 1, selector)
}
#[test]
fn later_batch_choice_stages_can_read_only_preceding_public_scalar_answers() {
    let mut batch = simultaneous(
        vec![instruction(
            Action::Reveal {
                objects: Selector::Bound {
                    reference: Binding::new(2, ScopeId(1)),
                },
            },
            1,
            "synthetic",
        )],
        3,
    );
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch.node
    {
        batch.choice_stages = vec![
            BatchChoiceStage {
                decisions: vec![choose(1, 1)],
                order: PlayerOrder::TurnPlayerFirst,
            },
            BatchChoiceStage {
                decisions: vec![answer_dependent_choice()],
                order: PlayerOrder::TurnPlayerFirst,
            },
        ];
    }
    accept(body(1, vec![batch.clone()]));
    let mut private = batch.clone();
    if let Node::Control {
        control: Control::Simultaneous {
            batch: private_batch,
            ..
        },
    } = &mut private.node
    {
        private_batch.choice_stages[0].decisions[0].visibility = Visibility::Only {
            players: controller_set(),
        };
    }
    reject(body(1, vec![private]), WellFormedReason::UnsupportedShape);
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch.node
    {
        batch.choice_stages = vec![BatchChoiceStage {
            decisions: vec![choose(1, 1), answer_dependent_choice()],
            order: PlayerOrder::TurnPlayerFirst,
        }];
    }
    reject(body(1, vec![batch]), WellFormedReason::UnavailableBinding);
}
