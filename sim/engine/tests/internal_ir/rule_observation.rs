use super::*;
use sve_engine::game::internal::ResumeStage;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{CardState, HistoryEvent, ObservedZone};

pub(super) fn field(game: &Game, seat: Viewer, position: usize) -> CardState {
    game.observe(seat)
        .unwrap()
        .players
        .iter()
        .find(|player| player.player == seat)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards[position]
        .card
        .properties
        .state
        .clone()
        .unwrap()
}

fn board(hidden: &str) -> Game {
    let mut setup = setup(1, json!([]));
    setup["players"]["P1"]["zones"]["hand"] = json!([]);
    setup["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    setup["players"]["P2"]["zones"]["field"] = json!([]);
    setup["players"]["P2"]["zones"]["hand"] = json!([{"id":"secret","card":hidden}]);
    let mut game = Game::new(
        catalog(),
        &setup,
        &Value::Null,
        &Value::Null,
        "rule-observation",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

#[test]
fn public_history_is_read_only_and_does_not_depend_on_hidden_cards() {
    let mut game = board("unit-spell");
    let mut other = board("unit-amulet");
    let attack = json!({"do":"attack","attacker":"s","target":"P2.leader"});
    game.internal_rule_action(&attack).unwrap();
    other.internal_rule_action(&attack).unwrap();
    for board in [&mut game, &mut other] {
        while let Some(question) = board.internal_quick_choice().unwrap() {
            board
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
        }
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
    }
    let before = exact_authority(&game);
    let observation = game.observe(Viewer::P1).unwrap();
    assert_eq!(observation, other.observe(Viewer::P1).unwrap());
    assert_eq!(observation, game.observe(Viewer::P1).unwrap());
    assert_eq!(before, exact_authority(&game));
    let attack = observation
        .history
        .subjects
        .iter()
        .find(|fact| fact.event == HistoryEvent::Attack)
        .unwrap();
    assert!(attack.current.is_some());
    assert_eq!(attack.controller, Viewer::P1);
    let banish = checked(vec![instruction(
        1,
        Action::Banish {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
    )]);
    game.execute_internal(&banish, &source("synthetic"), context(1))
        .unwrap();
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .history
            .subjects
            .iter()
            .filter(|fact| fact.event == HistoryEvent::Attack)
            .all(|fact| fact.current.is_none())
    );
    let restore = checked(vec![instruction(
        2,
        Action::Move {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    },
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
    )]);
    let mut returned = context(1);
    returned.generation = Generation(1);
    game.execute_internal(&restore, &source("synthetic"), returned)
        .unwrap();
    let after = game.observe(Viewer::P1).unwrap();
    assert!(
        after
            .history
            .subjects
            .iter()
            .filter(|fact| fact.event == HistoryEvent::Attack)
            .all(|fact| fact.current.is_none())
    );
    assert_eq!(field(&game, Viewer::P1, 0).posture, Posture::Standing);
}

#[test]
fn a_concealed_prevention_source_does_not_disclose_its_text_or_private_effect_count() {
    let mut first = board("unit-spell");
    let mut second = board("unit-amulet");
    let program = checked(vec![next_damage::install(1, false)]);
    for game in [&mut first, &mut second] {
        let mut hidden = context(2);
        hidden.source = "secret".into();
        game.execute_internal(&program, &source("synthetic"), hidden)
            .unwrap();
    }
    let observation = first.observe(Viewer::P1).unwrap();
    assert_eq!(observation, second.observe(Viewer::P1).unwrap());
    assert_eq!(
        observation
            .players
            .iter()
            .find(|player| player.player == Viewer::P2)
            .unwrap()
            .effects
            .len(),
        0
    );
    let own = first.observe(Viewer::P2).unwrap();
    assert_eq!(
        own.players
            .iter()
            .find(|player| player.player == Viewer::P2)
            .unwrap()
            .effects
            .len(),
        1
    );
}

#[test]
fn installed_attack_restriction_matches_the_authoritative_leader_gate() {
    use sve_engine::game::player::observation::{EffectExpiry, InstalledChange};
    let mut game = board("unit-spell");
    let affected = Binding::new(1, ScopeId(90));
    let targets = Selector::Bound {
        reference: affected,
    };
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let program = checked(vec![instruction(
        1,
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Permission {
                subject: PermissionSubject::Objects {
                    objects: targets.clone(),
                },
                action: ActionPattern::Attack {
                    attacker: targets,
                    target: Some(RecipientSet::Leaders {
                        players: players(PlayerRelation::Opponents),
                    }),
                },
                polarity: Polarity::Forbid,
                condition: Expr::Literal(Bool(true)),
                duration: duration.clone(),
                rule: RuleId(1),
            }),
            duration,
        },
    )]);
    game.execute_internal(&program, &source("synthetic"), context(1))
        .unwrap();
    let state = field(&game, Viewer::P1, 0);
    let effect = state
        .effects
        .iter()
        .find(|effect| matches!(effect.change, InstalledChange::AttackRestriction { .. }))
        .unwrap();
    assert_eq!(
        effect.change,
        InstalledChange::AttackRestriction {
            leaders: Some(vec![Viewer::P2])
        }
    );
    assert_eq!(effect.expires, EffectExpiry::TurnEnd);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
}
