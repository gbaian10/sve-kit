use super::*;
use core::array::from_fn;
use sve_engine::game::player::events::{ActionKind, EventKind};
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{DeckConstruction, Input, PlayerDeck, Viewer};
use sve_engine::host::{HostLimits, HostOutcome, PolicyHost};

#[test]
fn policy_host_real_game_plays_attacks_and_resolves_triggers_until_leader_defeat() {
    // A whole match exercises opening, both seats and trigger resumption through the same host.
    // Repeated printings isolate the player protocol; tournament construction is a host prerequisite.
    let decks = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: vec!["BP10-122".into(); 40],
        evolve: Vec::new(),
    });
    let mut game =
        Game::start_player_game(selection().clone(), decks, Viewer::P2, "policy-whole-game")
            .unwrap();
    assert!(
        matches!(game.input(Viewer::P1), Input::Waiting(waiting) if waiting.actor == Viewer::P2)
    );
    let mut host = PolicyHost::new(HostLimits::default());
    let report = host.run(&mut game);
    assert!(
        matches!(report.outcome, HostOutcome::Finished { winner: Some(_) }),
        "{report:?}"
    );
    assert_eq!(report.stats.rejected, 0);
    assert_eq!(report.stats.engine_faults, 0);
    assert!(report.stats.applied_by_seat.iter().all(|count| *count > 5));
    for viewer in [Viewer::P1, Viewer::P2] {
        assert!(matches!(
            game.input(viewer),
            Input::Finished { winner: Some(_) }
        ));
        let observation = game.observe(viewer).unwrap();
        assert!(
            observation
                .players
                .iter()
                .any(|player| player.resources.life <= 0)
        );
        assert!(observation.players.iter().all(|player| {
            player
                .zones
                .iter()
                .find(|zone| zone.kind == ObservedZone::Deck)
                .unwrap()
                .count
                > 0
        }));
        let events = game.events(viewer, None).unwrap().collect::<Vec<_>>();
        for kind in [
            ActionKind::Play,
            ActionKind::Attack,
            ActionKind::AbilityResolve,
        ] {
            for actor in [Viewer::P1, Viewer::P2] {
                assert!(events.iter().any(|event| matches!(&event.kind,
                    EventKind::Action(action) if action.kind == kind && action.player == Some(actor))),
                    "Both seats perform {kind:?}");
            }
        }
        assert!(events.iter().any(|event| matches!(&event.kind,
            EventKind::Result { rule: Some(rule), .. } if rule == "rule-11.2.1")));
        assert!(!events.iter().any(|event| matches!(&event.kind,
            EventKind::Result { rule: Some(rule), .. } if rule == "rule-1.2.3")));
    }
}
