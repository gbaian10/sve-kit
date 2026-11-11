use super::*;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{
    CardObservation, EffectExpiry, InstalledChange, InstalledEffect, NumberChange, Observation,
    ObservedZone,
};

fn observed_card(
    observation: &Observation,
    player: Viewer,
    zone: ObservedZone,
) -> &CardObservation {
    &observation
        .players
        .iter()
        .find(|seat| seat.player == player)
        .unwrap()
        .zones
        .iter()
        .find(|area| area.kind == zone)
        .unwrap()
        .cards[0]
        .card
}

fn board(public_source: bool, source_card: &str) -> Game {
    let mut initial = setup(1, json!([{"id":"draw-one","card":"unit-spell"}]));
    let zone = if public_source { "field" } else { "hand" };
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":source_card}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw-two","card":"unit-spell"}]);
    Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "effect-observation",
    )
    .unwrap()
}

fn apply(game: &mut Game, steps: Vec<Located<Node>>) {
    assert!(
        game.execute_internal(&checked(steps), &source("synthetic"), context(1))
            .unwrap()
            .complete,
        "fixture must finish without a question"
    );
}

#[test]
fn numeric_effects_explain_effective_values_and_expire_with_the_authority() {
    let mut game = board(true, "unit-follower");
    apply(
        &mut game,
        vec![numeric::modify(
            1,
            zone(ZoneKind::Field, 91),
            numeric::pair(2, 1),
            DurationEnd::CurrentTurnEnd {},
        )],
    );
    let before = format!("{game:?}");
    let observation = game.observe(Viewer::P2).unwrap();
    assert_eq!(before, format!("{game:?}"));
    let card = observed_card(&observation, Viewer::P2, ObservedZone::Field);
    let state = card.properties.state.as_ref().unwrap();
    assert_eq!(state.stats.as_ref().unwrap().attack, 4);
    assert_eq!(state.stats.as_ref().unwrap().life, 4);
    assert_eq!(state.effects.len(), 1);
    let effect = &state.effects[0];
    assert_eq!(effect.expires, EffectExpiry::TurnEnd);
    assert!(effect.active);
    assert_eq!(
        effect.change,
        InstalledChange::Numeric {
            attack: Some(NumberChange::Add(2)),
            life: Some(NumberChange::Add(1))
        }
    );
    let origin = effect.source.as_ref().unwrap();
    assert_eq!(origin.card, "unit-follower");
    assert_eq!(origin.line, Some(1));
    assert_eq!(
        origin.holder.as_ref(),
        Some(&observed_card(&observation, Viewer::P1, ObservedZone::Field).handle)
    );
    game.decide(&json!({"do":"end-phase"}), "end").unwrap();
    game.decide(&json!({"do":"pass"}), "pass").unwrap();
    let after = game.observe(Viewer::P2).unwrap();
    let state = observed_card(&after, Viewer::P2, ObservedZone::Field)
        .properties
        .state
        .as_ref()
        .unwrap();
    assert_eq!(state.effects, Vec::<InstalledEffect>::new());
    assert_eq!(state.stats.as_ref().unwrap().attack, 2);
    assert_eq!(state.stats.as_ref().unwrap().life, 3);
}

#[test]
fn a_concealed_installer_does_not_disclose_its_card_through_public_numeric_layers() {
    let mut first = board(false, "unit-spell");
    let mut second = board(false, "unit-amulet");
    for game in [&mut first, &mut second] {
        apply(game, vec![numeric::buff(1, 2, 0)]);
    }
    assert_eq!(
        first.observe(Viewer::P2).unwrap(),
        second.observe(Viewer::P2).unwrap()
    );
    let observation = first.observe(Viewer::P2).unwrap();
    assert!(
        observed_card(&observation, Viewer::P2, ObservedZone::Field)
            .properties
            .state
            .as_ref()
            .unwrap()
            .effects[0]
            .source
            .is_none()
    );
    let own = first.observe(Viewer::P1).unwrap();
    assert_eq!(
        observed_card(&own, Viewer::P2, ObservedZone::Field)
            .properties
            .state
            .as_ref()
            .unwrap()
            .effects[0]
            .source
            .as_ref()
            .unwrap()
            .card,
        "unit-spell"
    );
}

#[test]
fn effect_source_memory_does_not_relink_after_the_holder_changes_generation() {
    let mut game = board(true, "unit-follower");
    apply(
        &mut game,
        vec![
            numeric::buff(1, 2, 0),
            movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Hand,
            ),
        ],
    );
    let observation = game.observe(Viewer::P2).unwrap();
    let source = observed_card(&observation, Viewer::P2, ObservedZone::Field)
        .properties
        .state
        .as_ref()
        .unwrap()
        .effects[0]
        .source
        .as_ref()
        .unwrap();
    assert_eq!(source.card, "unit-follower");
    assert!(source.holder.is_none());
    let own = game.observe(Viewer::P1).unwrap();
    assert!(
        observed_card(&own, Viewer::P2, ObservedZone::Field)
            .properties
            .state
            .as_ref()
            .unwrap()
            .effects[0]
            .source
            .as_ref()
            .unwrap()
            .holder
            .is_none()
    );
}

#[test]
fn an_expired_reveal_does_not_relink_an_effect_source_after_reidentification() {
    let mut game = board(false, "unit-spell");
    let reveal = instruction(
        1,
        Action::Reveal {
            objects: Selector::Zone {
                players: PlayerSet {
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    ..players(PlayerRelation::SelfPlayer)
                },
                zone: ZoneKind::Hand,
                view: SelectionView::Current {},
                binding: Binding::new(90, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        },
    );
    let order = || {
        vec![
            ordering::look(3, 1),
            ordering::order(4, ordering::looked(3), Position::Top {}),
        ]
    };
    let mut steps = vec![reveal.clone(), numeric::buff(2, 2, 0)];
    steps.extend(order());
    game.execute_internal(&checked(steps), &source("synthetic"), context(1))
        .unwrap();
    let initial = game.observe(Viewer::P2).unwrap();
    let original = observed_card(&initial, Viewer::P2, ObservedZone::Field)
        .properties
        .state
        .as_ref()
        .unwrap()
        .effects[0]
        .source
        .as_ref()
        .unwrap()
        .holder
        .clone()
        .unwrap();
    let question = game.internal_decision(View::P1).unwrap();
    game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
        .unwrap();
    let mut again = vec![reveal];
    again.extend(order());
    game.execute_internal(&checked(again), &source("synthetic"), context(1))
        .unwrap();
    let later = game.observe(Viewer::P2).unwrap();
    assert_ne!(
        &observed_card(&later, Viewer::P1, ObservedZone::Hand).handle,
        &original
    );
    let effect = &observed_card(&later, Viewer::P2, ObservedZone::Field)
        .properties
        .state
        .as_ref()
        .unwrap()
        .effects[0];
    assert_eq!(effect.source.as_ref().unwrap().card, "unit-spell");
    assert!(effect.source.as_ref().unwrap().holder.is_none());
}
