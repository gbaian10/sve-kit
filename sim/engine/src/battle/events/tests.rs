use super::*;
use crate::game::player::ObjectHandle;
use crate::game::player::observation::{CardProperties, TextProvenance};
use sve_battle_contract::CardRef;

/// Stands in for the board projection: a native card number is its printing, and a native
/// handle is reused as the wire object.
struct Board;

impl EventObjects for Board {
    fn card(&self, card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        Ok(wire::EventCard {
            card: CardRef {
                printing_id: card.properties.card.clone(),
                face: Nat::try_from(card.properties.face)?,
            },
            object: card
                .handle
                .as_ref()
                .map(|handle| sve_battle_contract::ObjectHandle::new(handle.as_str().to_owned())),
        })
    }

    fn source(&self, source: &EffectSource) -> Result<SourceRef, ProjectionError> {
        Ok(SourceRef::Card {
            printing_id: source.card.clone(),
            face: None,
            line: source.line.map(Nat::try_from).transpose()?,
            holder: None,
        })
    }

    fn turn(&self, turn: &TurnObservation) -> Result<TurnView, ProjectionError> {
        let [first, second] = turn.elapsed_turns;
        Ok(TurnView {
            active: turn.active.map(seat),
            first_player: turn.first_player.map(seat),
            phase: None,
            elapsed_turns: [Nat::try_from(first)?, Nat::try_from(second)?],
        })
    }
}

fn card(number: &str, handle: Option<&str>) -> EventCard {
    EventCard {
        handle: handle.map(|text| ObjectHandle(text.to_owned())),
        properties: CardProperties {
            card: number.to_owned(),
            face: 0,
            names: vec![number.to_owned()],
            kind: "follower".to_owned(),
            traits: Vec::new(),
            owner: Viewer::P1,
            controller: Viewer::P1,
            state: None,
        },
    }
}

fn subject(number: &str, handle: Option<&str>) -> EventSubject {
    EventSubject::Card(Box::new(card(number, handle)))
}

fn wire_card(number: &str, handle: Option<&str>) -> wire::EventCard {
    wire::EventCard {
        card: CardRef {
            printing_id: number.to_owned(),
            face: Nat::try_from(0_u64).unwrap(),
        },
        object: handle.map(|text| sve_battle_contract::ObjectHandle::new(text.to_owned())),
    }
}

fn wire_subject(number: &str, handle: Option<&str>) -> Subject {
    let projected = wire_card(number, handle);
    Subject::Card {
        card: projected.card,
        object: projected.object,
    }
}

fn action(kind: ActionKind) -> ActionEvent {
    ActionEvent {
        kind,
        player: None,
        subjects: Vec::new(),
        source: None,
        numbers: Vec::new(),
        name: None,
        face_up: None,
        details: None,
    }
}

fn facet(amount: i64, before: Option<i64>, after: Option<i64>) -> NumberFacet {
    NumberFacet {
        stat: None,
        before,
        after,
        amount,
    }
}

fn one(kind: &EventKind) -> Result<ProjectedEvent, ProjectionError> {
    project_batch([kind], &Board).map(|mut events| events.remove(0))
}

fn done(kind: ActionEvent) -> Result<ProjectedEvent, ProjectionError> {
    one(&EventKind::Action(kind))
}

fn number(value: u64) -> Nat {
    Nat::try_from(value).unwrap()
}

fn signed(value: i64) -> Int {
    Int::try_from(value).unwrap()
}

#[test]
fn a_movement_keeps_its_method_whether_an_effect_moved_it_and_each_end_as_authorized() {
    let location = |zone, held: Option<EventCard>| EventLocation {
        player: Viewer::P1,
        zone: Some(zone),
        position: Some(2),
        card: held,
    };
    let moved = |method, known: bool| EventKind::Movement {
        method,
        before: Box::new(location(ObservedZone::Deck, None)),
        after: Box::new(location(
            ObservedZone::Hand,
            known.then(|| card("drawn", Some("h1"))),
        )),
    };
    let owner = one(&moved(MovementMethod::Draw, true)).unwrap();
    let EventBody::Movement {
        method,
        by_effect,
        before,
        after,
    } = &owner.body
    else {
        panic!("movement body");
    };
    assert_eq!(
        (*method, *by_effect),
        (wire::MovementMethod::Draw, false),
        "a rule draw is not an effect instruction"
    );
    assert_eq!(
        (before.zone, before.card.clone(), before.position),
        (Some(Zone::Deck), None, Some(number(2))),
        "the unknown deck end has no card"
    );
    assert_eq!(
        after.card,
        Some(wire_card("drawn", Some("h1"))),
        "the owner sees the drawn card"
    );
    assert_eq!(
        owner.about,
        vec![wire_subject("drawn", Some("h1"))],
        "an identifiable card can be highlighted"
    );

    let opponent = one(&moved(MovementMethod::Draw, false)).unwrap();
    assert!(
        matches!(&opponent.body, EventBody::Movement { after: hand, .. } if hand.card.is_none())
            && opponent.about.is_empty(),
        "an unknown draw names no card and highlights nothing"
    );
    for (native, expected) in [
        (
            MovementMethod::Effect(MoveMethod::Destroy),
            (wire::MovementMethod::Destroy, true),
        ),
        (
            MovementMethod::RuleDestroy,
            (wire::MovementMethod::Destroy, false),
        ),
        (
            MovementMethod::Procedure,
            (wire::MovementMethod::Procedure, false),
        ),
        (
            MovementMethod::Effect(MoveMethod::Banish),
            (wire::MovementMethod::Banish, true),
        ),
        (
            MovementMethod::Effect(MoveMethod::Draw),
            (wire::MovementMethod::Draw, true),
        ),
    ] {
        let projected = one(&moved(native, false)).unwrap();
        assert!(
            matches!(projected.body, EventBody::Movement { method: kept, by_effect: effect, .. }
                if (kept, effect) == expected),
            "{native:?} keeps its rules method and who performed it"
        );
    }
}

#[test]
fn damage_names_source_then_target_with_its_class_amount_and_resulting_life() {
    let mut dealt = action(ActionKind::Damage);
    dealt.player = Some(Viewer::P1);
    dealt.subjects = vec![
        Some(subject("spell", None)),
        Some(EventSubject::Leader(Viewer::P2)),
    ];
    dealt.numbers = vec![facet(3, Some(20), Some(17))];
    dealt.details = Some(ActionDetails::Damage(DamageFlags::Ability));
    let projected = done(dealt.clone()).unwrap();
    assert_eq!(
        projected.body,
        EventBody::Damage {
            source: Some(wire_subject("spell", None)),
            target: Some(Subject::Leader { player: Seat::P2 }),
            class: wire::DamageClass::Ability,
            amount: number(3),
            life_after: Some(signed(17)),
        },
        "the source may be a remembered face; the target is the damaged leader"
    );
    assert_eq!(
        projected.about,
        vec![Subject::Leader { player: Seat::P2 }],
        "a remembered face has no object to highlight"
    );

    dealt.details = None;
    assert_eq!(
        done(dealt),
        Err(ProjectionError::Unavailable),
        "damage without its rules class is not guessed"
    );
}

#[test]
fn plays_and_resolutions_require_the_seat_the_producer_recorded() {
    let mut played = action(ActionKind::Play);
    played.subjects = vec![Some(subject("card", Some("h1")))];
    assert_eq!(
        done(played.clone()),
        Err(ProjectionError::Unavailable),
        "a play without its player is a producer gap, not a guess from the board"
    );
    played.player = Some(Viewer::P2);
    assert_eq!(
        done(played).unwrap().body,
        EventBody::CardPlayed {
            player: Seat::P2,
            card: Some(wire_card("card", Some("h1"))),
        },
        "the card play names its player and card"
    );

    let mut ability = action(ActionKind::AbilityPlay);
    ability.player = Some(Viewer::P1);
    ability.subjects = vec![Some(subject("holder", Some("h2")))];
    ability.source = Some(EffectSource {
        provenance: TextProvenance::Unknown,
        card: "holder".to_owned(),
        face: None,
        line: Some(2),
        wording: None,
        holder: None,
    });
    ability.details = Some(ActionDetails::Ability(AbilityQualification::UnionBurst));
    assert_eq!(
        done(ability).unwrap().body,
        EventBody::AbilityPlayed {
            player: Seat::P1,
            holder: Some(wire_subject("holder", Some("h2"))),
            source: Some(SourceRef::Card {
                printing_id: "holder".to_owned(),
                face: None,
                line: Some(number(2)),
                holder: None,
            }),
            ability: None,
            qualification: wire::AbilityQualification::UnionBurst,
        },
        "an ability play keeps its holder, public text origin and qualification"
    );
}

#[test]
fn resource_changes_name_the_resource_and_the_seat_from_the_player_or_the_leader() {
    let mut pp = action(ActionKind::Resource);
    pp.player = Some(Viewer::P1);
    pp.name = Some("pp".to_owned());
    pp.numbers = vec![facet(2, None, None)];
    assert_eq!(
        done(pp).unwrap().body,
        EventBody::ResourceChanged {
            player: Seat::P1,
            resource: wire::Resource::Pp,
            change: wire::ValueChange {
                before: None,
                after: None,
                delta: signed(2),
            },
        },
        "a PP recovery has only its amount"
    );

    let mut life = action(ActionKind::Resource);
    life.name = Some("life".to_owned());
    life.subjects = vec![Some(EventSubject::Leader(Viewer::P2))];
    life.numbers = vec![facet(3, None, None)];
    let healed = done(life).unwrap();
    assert!(
        matches!(
            healed.body,
            EventBody::ResourceChanged {
                player: Seat::P2,
                resource: wire::Resource::Life,
                ..
            }
        ) && healed.about == vec![Subject::Leader { player: Seat::P2 }],
        "a life gain takes its seat from the leader it names"
    );

    let mut unknown = action(ActionKind::Resource);
    unknown.player = Some(Viewer::P1);
    unknown.name = Some("mana".to_owned());
    unknown.numbers = vec![facet(1, None, None)];
    assert_eq!(
        done(unknown),
        Err(ProjectionError::Unavailable),
        "an unknown resource name is not mapped by guess"
    );
}

#[test]
fn evolution_keeps_the_form_state_and_whether_an_effect_did_it() {
    for (kind, role) in [
        (ActionKind::Evolve, wire::EvolutionRole::Normal),
        (ActionKind::EffectEvolve, wire::EvolutionRole::Effect),
    ] {
        let mut evolved = action(kind);
        evolved.player = Some(Viewer::P1);
        evolved.subjects = vec![
            Some(subject("base", Some("h1"))),
            Some(subject("form", Some("h2"))),
        ];
        evolved.details = Some(ActionDetails::Evolution(EvolutionState::Evolved));
        assert_eq!(
            done(evolved).unwrap().body,
            EventBody::Evolved {
                subject: Some(wire_subject("base", Some("h1"))),
                form: Some(wire_card("form", Some("h2"))),
                state: observation::EvolutionState::Evolved,
                role,
            },
            "the follower, the evolve card and the rules role are kept"
        );
    }
    let mut supered = action(ActionKind::Evolve);
    supered.subjects = vec![Some(subject("base", Some("h1")))];
    supered.details = Some(ActionDetails::Evolution(EvolutionState::SuperEvolved));
    assert!(
        matches!(
            done(supered).unwrap().body,
            EventBody::Evolved {
                form: None,
                state: observation::EvolutionState::SuperEvolved,
                ..
            }
        ),
        "a super evolution places no new card"
    );
}

#[test]
fn a_result_names_the_winner_and_distinguishes_concession_defeat_and_a_shared_defeat() {
    let lost = |loser, rule: &str| EventKind::Result {
        winner: None,
        loser: Some(loser),
        rule: Some(rule.to_owned()),
    };
    let reason = |projected: &ProjectedEvent| {
        let EventBody::Result {
            winner,
            loser,
            reason,
            ..
        } = &projected.body
        else {
            panic!("result body, got {:?}", projected.body);
        };
        (*winner, *loser, *reason)
    };
    assert_eq!(
        reason(&one(&lost(Viewer::P2, "rule-1.2.3")).unwrap()),
        (
            Some(Seat::P1),
            Some(Seat::P2),
            wire::ResultReason::Concession
        ),
        "a concession makes the other player win"
    );
    assert_eq!(
        reason(&one(&lost(Viewer::P1, "rule-11.2.1")).unwrap()),
        (Some(Seat::P2), Some(Seat::P1), wire::ResultReason::Defeat),
        "a single defeat makes the other player win"
    );
    let both = [
        lost(Viewer::P1, "rule-11.2.1"),
        lost(Viewer::P2, "rule-11.2.2"),
    ];
    let together = project_batch(&both, &Board).unwrap();
    assert_eq!(
        together.iter().map(reason).collect::<Vec<_>>(),
        vec![
            (None, Some(Seat::P1), wire::ResultReason::SimultaneousDefeat),
            (None, Some(Seat::P2), wire::ResultReason::SimultaneousDefeat),
        ],
        "two losers in one batch lost together and nobody won"
    );
    let won = EventKind::Result {
        winner: Some(Viewer::P1),
        loser: None,
        rule: None,
    };
    assert_eq!(
        reason(&one(&won).unwrap()),
        (Some(Seat::P1), None, wire::ResultReason::Effect),
        "an effect victory names only its winner"
    );
}

#[test]
fn a_shared_effect_win_names_no_winner_and_a_mixed_ending_is_refused() {
    let won = |winner| EventKind::Result {
        winner: Some(winner),
        loser: None,
        rule: None,
    };
    let bodies = project_batch(&[won(Viewer::P1), won(Viewer::P2)], &Board)
        .unwrap()
        .into_iter()
        .map(|event| event.body)
        .collect::<Vec<_>>();
    let drawn = EventBody::Result {
        winner: None,
        loser: None,
        reason: wire::ResultReason::Effect,
        rule: None,
    };
    assert_eq!(
        bodies,
        vec![drawn.clone(), drawn],
        "two players made to win in one batch drew: neither is named the winner"
    );

    let mixed = [
        won(Viewer::P1),
        EventKind::Result {
            winner: None,
            loser: Some(Viewer::P1),
            rule: Some("rule-11.2.1".to_owned()),
        },
    ];
    assert_eq!(
        project_batch(&mixed, &Board),
        Err(ProjectionError::Unavailable),
        "a win and a defeat in one batch could name different winners"
    );
}

#[test]
fn results_the_native_trace_cannot_support_are_refused_instead_of_filled_in() {
    let mut control = action(ActionKind::Control);
    control.subjects = vec![Some(subject("equipment", Some("h1")))];
    assert_eq!(
        done(control),
        Err(ProjectionError::Unavailable),
        "a control change without owner and controllers is not invented"
    );

    let mut counter = action(ActionKind::Counter);
    counter.subjects = vec![Some(subject("card", Some("h1")))];
    counter.name = Some(String::new());
    counter.numbers = vec![facet(1, None, None)];
    assert_eq!(
        done(counter),
        Err(ProjectionError::Unavailable),
        "a counter without a rules name is not shown under an empty one"
    );

    let mut huge = action(ActionKind::Dice);
    huge.player = Some(Viewer::P1);
    huge.numbers = vec![facet(i64::MAX, None, None)];
    assert_eq!(
        done(huge),
        Err(ProjectionError::Range),
        "a number beyond the wire range stops the projection instead of being rounded"
    );
}

#[test]
fn about_lists_each_named_leader_and_identifiable_card_once_in_order() {
    let mut attack = action(ActionKind::Attack);
    attack.player = Some(Viewer::P1);
    attack.subjects = vec![
        Some(subject("attacker", Some("h1"))),
        Some(subject("attacker", Some("h1"))),
    ];
    let projected = done(attack).unwrap();
    assert_eq!(
        projected.about,
        vec![wire_subject("attacker", Some("h1"))],
        "the same card named twice is highlighted once"
    );

    let mut equipped = action(ActionKind::Equip);
    equipped.player = Some(Viewer::P1);
    equipped.subjects = vec![
        Some(subject("bearer", Some("h1"))),
        Some(subject("gear", Some("h2"))),
    ];
    let worn = done(equipped).unwrap();
    assert_eq!(
        worn.body,
        EventBody::Equipped {
            equipment: Some(wire_subject("gear", Some("h2"))),
            bearer: Some(wire_subject("bearer", Some("h1"))),
        },
        "the holder is recorded first and the equipment second"
    );
    assert_eq!(
        worn.about,
        vec![
            wire_subject("gear", Some("h2")),
            wire_subject("bearer", Some("h1"))
        ],
        "subjects are listed in the order the body names them"
    );
}

/// An action of `kind` by P1 naming one identifiable card.
fn by_first(kind: ActionKind) -> ActionEvent {
    let mut event = action(kind);
    event.player = Some(Viewer::P1);
    event.subjects = vec![Some(subject("card", Some("h1")))];
    event
}

/// `event` after `adjust` filled in the facts one case needs.
fn with(mut event: ActionEvent, adjust: &dyn Fn(&mut ActionEvent)) -> ActionEvent {
    adjust(&mut event);
    event
}

/// The wire subject `by_first` names.
fn named() -> Subject {
    wire_subject("card", Some("h1"))
}

/// The wire card `by_first` names.
fn held() -> wire::EventCard {
    wire_card("card", Some("h1"))
}

/// Projects each native action alone and compares the whole wire body.
fn assert_bodies(cases: Vec<(ActionEvent, EventBody)>) {
    for (native, expected) in cases {
        let kind = native.kind;
        assert_eq!(
            done(native).unwrap().body,
            expected,
            "{kind:?} keeps its native facts in the wire body that names them"
        );
    }
}

#[test]
fn resolutions_and_value_changes_keep_their_native_facts() {
    assert_bodies(vec![
        (
            by_first(ActionKind::Resolve),
            EventBody::CardResolved {
                player: Seat::P1,
                card: Some(held()),
            },
        ),
        (
            by_first(ActionKind::AbilityResolve),
            EventBody::AbilityResolved {
                player: Seat::P1,
                holder: Some(named()),
                source: None,
                ability: None,
            },
        ),
        (
            with(action(ActionKind::Life), &|event| {
                event.player = Some(Viewer::P2);
                event.numbers = vec![facet(-2, Some(20), Some(18))];
            }),
            EventBody::LifeChanged {
                player: Seat::P2,
                change: wire::ValueChange {
                    before: Some(signed(20)),
                    after: Some(signed(18)),
                    delta: signed(-2),
                },
            },
        ),
        (
            with(by_first(ActionKind::Stats), &|event| {
                event.numbers = vec![NumberFacet {
                    stat: Some(Stat::Attack),
                    before: Some(2),
                    after: Some(3),
                    amount: 1,
                }];
            }),
            EventBody::StatsChanged {
                subject: Some(named()),
                changes: vec![wire::StatChange {
                    stat: wire::Stat::Attack,
                    change: wire::ValueChange {
                        before: Some(signed(2)),
                        after: Some(signed(3)),
                        delta: signed(1),
                    },
                }],
            },
        ),
        (
            with(by_first(ActionKind::Counter), &|event| {
                event.name = Some("stack".to_owned());
                event.numbers = vec![facet(-1, None, None)];
            }),
            EventBody::CounterChanged {
                subject: Some(named()),
                name: "stack".to_owned(),
                change: wire::ValueChange {
                    before: None,
                    after: None,
                    delta: signed(-1),
                },
            },
        ),
    ]);
}

#[test]
fn single_subject_state_changes_map_to_their_own_bodies() {
    assert_bodies(vec![
        (
            by_first(ActionKind::Act),
            EventBody::PostureChanged {
                subject: Some(named()),
                posture: Posture::Acted,
            },
        ),
        (
            by_first(ActionKind::Stand),
            EventBody::PostureChanged {
                subject: Some(named()),
                posture: Posture::Standing,
            },
        ),
        (
            by_first(ActionKind::Reveal),
            EventBody::Revealed {
                cards: vec![held()],
            },
        ),
        (
            with(by_first(ActionKind::Facing), &|event| {
                event.face_up = Some(false);
            }),
            EventBody::FacingChanged {
                subject: Some(named()),
                facing: Facing::Down,
            },
        ),
        (
            by_first(ActionKind::Destroy),
            EventBody::DestroyAttempted {
                subject: Some(named()),
            },
        ),
        (
            by_first(ActionKind::Banish),
            EventBody::BanishAttempted {
                subject: Some(named()),
            },
        ),
        (
            by_first(ActionKind::Replacement),
            EventBody::ReplacementApplied {
                subject: Some(named()),
                source: None,
            },
        ),
        (
            by_first(ActionKind::Erase),
            EventBody::Erased {
                subject: Some(named()),
            },
        ),
        (
            with(by_first(ActionKind::Control), &|event| {
                event.player = None;
                event.details = Some(ActionDetails::Control {
                    owner: Viewer::P2,
                    before: Viewer::P2,
                    after: Viewer::P1,
                });
            }),
            EventBody::ControlChanged {
                subject: Some(named()),
                owner: Seat::P2,
                before: Seat::P2,
                after: Seat::P1,
            },
        ),
    ]);
}

#[test]
fn fusion_race_and_drive_keep_their_numbers_and_details() {
    assert_bodies(vec![
        (
            with(by_first(ActionKind::Fusion), &|event| {
                event.numbers = vec![facet(2, None, None)];
                event.details = Some(ActionDetails::Fusion(vec!["trait".to_owned()]));
            }),
            EventBody::Fusion {
                player: Seat::P1,
                subject: Some(named()),
                materials: number(2),
                material_traits: vec!["trait".to_owned()],
            },
        ),
        (
            with(by_first(ActionKind::Race), &|event| {
                event.numbers = vec![facet(3, None, None)];
            }),
            EventBody::Race {
                player: Seat::P1,
                subject: Some(named()),
                ordinal: number(3),
            },
        ),
        (
            by_first(ActionKind::DriveGained),
            EventBody::DriveGained {
                player: Seat::P1,
                subject: Some(named()),
            },
        ),
        (
            with(by_first(ActionKind::DriveTrigger), &|event| {
                event.details = Some(ActionDetails::DriveTrigger(TriggerIcon::Heal));
            }),
            EventBody::DriveTrigger {
                player: Seat::P1,
                card: Some(held()),
                icon: question::TriggerIcon::Heal,
            },
        ),
        (
            with(by_first(ActionKind::DriveChecked), &|event| {
                event.details = Some(ActionDetails::DriveChecked {
                    accepted: true,
                    player: Viewer::P1,
                    zone: ObservedZone::Cemetery,
                });
            }),
            EventBody::DriveChecked {
                player: Seat::P1,
                card: Some(held()),
                accepted: true,
                destination: wire::ZoneRef {
                    player: Seat::P1,
                    zone: Zone::Cemetery,
                },
            },
        ),
    ]);
}

#[test]
fn payments_dice_declarations_shuffles_and_skipped_turns_name_their_seat() {
    assert_bodies(vec![
        (
            with(action(ActionKind::Payment), &|event| {
                event.player = Some(Viewer::P1);
                event.details = Some(ActionDetails::Payment {
                    time: PaymentTime::ResolutionChoice,
                    complete: true,
                    zero: false,
                    replaced: false,
                });
            }),
            EventBody::Payment {
                player: Seat::P1,
                time: wire::PaymentTime::ResolutionChoice,
                complete: true,
                zero: false,
                replaced: false,
                component: wire::PaymentComponent::General,
            },
        ),
        (
            with(action(ActionKind::Payment), &|event| {
                event.player = Some(Viewer::P1);
                event.name = Some("earth rite".to_owned());
                event.numbers = vec![facet(1, None, None)];
                event.details = Some(ActionDetails::Payment {
                    time: PaymentTime::PlayParameter,
                    complete: true,
                    zero: false,
                    replaced: true,
                });
            }),
            EventBody::Payment {
                player: Seat::P1,
                time: wire::PaymentTime::PlayParameter,
                complete: true,
                zero: false,
                replaced: true,
                component: wire::PaymentComponent::EarthRite {
                    keyword: "earth rite".to_owned(),
                    stacks: number(1),
                },
            },
        ),
        (
            with(action(ActionKind::Dice), &|event| {
                event.player = Some(Viewer::P2);
                event.numbers = vec![facet(5, None, None)];
            }),
            EventBody::Dice {
                player: Seat::P2,
                value: number(5),
            },
        ),
        (
            with(action(ActionKind::Declare), &|event| {
                event.player = Some(Viewer::P2);
                event.numbers = vec![facet(4, None, None)];
            }),
            EventBody::ChoiceDeclared {
                player: Seat::P2,
                declared: observation::Declared::Number {
                    role: observation::DeclaredNumber::Free,
                    value: signed(4),
                },
            },
        ),
        (
            with(action(ActionKind::Shuffle), &|event| {
                event.player = Some(Viewer::P2);
            }),
            EventBody::Shuffled {
                player: Seat::P2,
                zone: Zone::Deck,
            },
        ),
        (
            with(action(ActionKind::TurnSkipped), &|event| {
                event.player = Some(Viewer::P2);
                event.details = Some(ActionDetails::TurnSkipped { extra: true });
            }),
            EventBody::TurnSkipped {
                player: Seat::P2,
                extra: true,
            },
        ),
    ]);
}

#[test]
fn a_phase_start_carries_the_public_turn_state() {
    let turn = TurnObservation {
        active: Some(Viewer::P2),
        first_player: Some(Viewer::P1),
        phase: None,
        elapsed_turns: [3, 2],
    };
    assert_eq!(
        one(&EventKind::Phase(turn)).unwrap().body,
        EventBody::PhaseStarted {
            turn: TurnView {
                active: Some(Seat::P2),
                first_player: Some(Seat::P1),
                phase: None,
                elapsed_turns: [number(3), number(2)],
            },
        },
        "the turn state is taken from the board projection unchanged"
    );
}
