use super::*;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{CardState, ObservedZone};

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
    history_fixture::public_history_board(hidden).unwrap()
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
