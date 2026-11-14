//! Real cards whose decisions belong to a player other than the effect controller, driven
//! through the player interface.
#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected player questions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape,
    SelectionQuestion, SubmissionKind, TargetRoleAssignment, Viewer, Waiting,
};

fn loaded(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "opp-decision-cards",
    )
    .unwrap();
    game.advance_player_input();
    game
}

fn deck(prefix: &str, count: usize) -> Value {
    Value::Array(
        (0..count)
            .map(|index| json!({"id":format!("{prefix}{index}"),"card":"BP01-161"}))
            .collect(),
    )
}

/// P1 holds `source` (`card`) in its main phase with 7 PP; both decks hold six spells.
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = deck("d", 6);
    initial["players"]["P2"]["zones"]["deck"] = deck("e", 6);
    initial
}

const fn other(viewer: Viewer) -> Viewer {
    match viewer {
        Viewer::P1 => Viewer::P2,
        Viewer::P2 => Viewer::P1,
    }
}

/// `viewer` takes the first main action that `choose` accepts.
fn act(
    game: &mut Game,
    viewer: Viewer,
    choose: &dyn Fn(&MainIntentKind) -> Option<MainActionAnswer>,
) {
    let Input::Question(question) = game.input(viewer) else {
        panic!("{viewer:?} has the main actions");
    };
    let QuestionShape::ActionWindow(actions) = &question.shape else {
        panic!("{viewer:?} has the main actions");
    };
    let (intent, answer) = actions
        .intents
        .iter()
        .find_map(|intent| choose(&intent.kind).map(|answer| (intent.id.clone(), answer)))
        .unwrap();
    let command = Command::MainAction {
        token: question.token,
        intent,
        answer,
    };
    assert_eq!(
        game.submit_player(viewer, command).kind,
        SubmissionKind::Applied,
        "{viewer:?} takes the main action"
    );
}

fn play(game: &mut Game, card: &str) {
    act(game, Viewer::P1, &|kind| {
        matches!(kind, MainIntentKind::Play { card: held } if held.properties.card == card)
            .then_some(MainActionAnswer::None)
    });
}

fn end_phase(game: &mut Game) {
    act(game, Viewer::P1, &|kind| {
        matches!(kind, MainIntentKind::EndPhase).then_some(MainActionAnswer::None)
    });
}

/// Answers every question until a player has its main actions again, returning who answered
/// which kind of question. While one player decides, the other only sees it waiting.
fn settle(
    game: &mut Game,
    targets: &mut dyn FnMut(&Game, Viewer, &SelectionQuestion) -> Answer,
) -> Vec<(Viewer, &'static str)> {
    let mut asked = Vec::new();
    for _ in 0_u8..64_u8 {
        let Some((viewer, question)) = [Viewer::P1, Viewer::P2].into_iter().find_map(|viewer| {
            if let Input::Question(question) = game.input(viewer) {
                Some((viewer, *question))
            } else {
                None
            }
        }) else {
            panic!("nobody is asked: {:?}", game.input(Viewer::P1));
        };
        let shape = match &question.shape {
            QuestionShape::ActionWindow(_) => return asked,
            QuestionShape::Decision(shape) => shape,
            unexpected => panic!("unexpected question {unexpected:?}"),
        };
        assert_eq!(
            game.input(other(viewer)),
            Input::Waiting(Waiting {
                actor: viewer,
                dice: None
            }),
            "only {viewer:?} may answer"
        );
        let (kind, answer) = match shape {
            DecisionShape::Quick { .. } => ("quick", Answer::Quick(None)),
            DecisionShape::Pending(options) => ("pending", Answer::Pending(options[0].handle)),
            DecisionShape::ResolutionPayment => ("payment", Answer::OptionalCost(true)),
            DecisionShape::Targets(domain) => ("targets", targets(game, viewer, domain)),
            unexpected => panic!("unexpected decision {unexpected:?}"),
        };
        asked.push((viewer, kind));
        let command = Command::Answer {
            token: question.token,
            answer,
        };
        assert_eq!(
            game.submit_player(viewer, command).kind,
            SubmissionKind::Applied,
            "{kind} answered by {viewer:?}"
        );
    }
    panic!("the scenario did not settle");
}

/// Only target questions, in the order they were asked.
fn chooser(asked: &[(Viewer, &'static str)]) -> Vec<Viewer> {
    asked
        .iter()
        .filter(|(_, kind)| *kind == "targets")
        .map(|(viewer, _)| *viewer)
        .collect()
}

fn handle(domain: &SelectionQuestion, card: &str) -> u64 {
    domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap()
        .handle
}

fn sees(game: &Game, viewer: Viewer, card: &str) -> bool {
    let observation = game.observe(viewer).unwrap();
    format!("{observation:?}").contains(card)
}

fn ids(game: &Game, player: &str, zone: &str) -> Vec<String> {
    game.projection(View::Referee).unwrap()[player][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn bp01_161_each_player_draws_back_its_own_hand_and_the_caster_one_more() {
    let mut initial = position("BP01-161");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"m0","card":"BP04-092"}),
            json!({"id":"m1","card":"BP04-092"}),
        ]);
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"o0","card":"BP04-092"},
        {"id":"o1","card":"BP04-092"},
        {"id":"o2","card":"BP04-092"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, "BP01-161");
    let asked = settle(&mut game, &mut |_, viewer, domain| {
        panic!("BP01-161 asks nothing: {viewer:?} {domain:?}")
    });
    assert_eq!(chooser(&asked), Vec::<Viewer>::new(), "nobody chooses");
    assert_eq!(ids(&game, "P1", "hand").len(), 3, "two back and one more");
    assert_eq!(ids(&game, "P2", "hand").len(), 3, "its own three back");
    assert_eq!(ids(&game, "P1", "deck").len(), 5);
    assert_eq!(ids(&game, "P2", "deck").len(), 6);
}

#[test]
fn bp07_050_the_chosen_cards_player_privately_puts_an_amulet_from_its_hand() {
    let mut initial = position("BP07-050");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"b1","card":"SD02-002"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"follower","card":"BP04-092"},
        {"id":"amulet","card":"BP06-121"},
        {"id":"spell","card":"BP01-161"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, "BP07-050");
    let asked = settle(&mut game, &mut |board, viewer, domain| {
        if viewer == Viewer::P1 {
            return Answer::Targets(vec![handle(domain, "SD02-002")]);
        }
        assert_eq!(domain.members.len(), 2, "a follower or an amulet");
        assert!(sees(board, Viewer::P2, "BP06-121"), "its own hand");
        for hidden in ["BP04-092", "BP06-121"] {
            assert!(!sees(board, Viewer::P1, hidden), "P1 never sees {hidden}");
        }
        Answer::Targets(vec![handle(domain, "BP06-121")])
    });
    assert_eq!(chooser(&asked), [Viewer::P1, Viewer::P2]);
    assert_eq!(
        ids(&game, "P2", "deck").last().map(String::as_str),
        Some("b1")
    );
    assert_eq!(ids(&game, "P2", "field"), ["amulet"]);
}

#[test]
fn bp01_008_the_chosen_followers_player_puts_it_at_the_deck_bottom() {
    let mut initial = position("BP01-008");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"b1","card":"SD02-002"},
        {"id":"b2","card":"BP04-092"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, "BP01-008");
    let asked = settle(&mut game, &mut |_, viewer, domain| {
        let only = handle(domain, "SD02-002");
        if viewer == Viewer::P1 {
            return Answer::Targets(vec![only]);
        }
        let edge = |top: bool| {
            domain
                .roles
                .iter()
                .find(|role| match role.position {
                    Some(Position::Top {}) => top,
                    Some(Position::Bottom {}) => !top,
                    Some(Position::Index { .. }) | None => false,
                })
                .unwrap()
                .role
        };
        Answer::TargetsWithRoles {
            targets: vec![only],
            roles: vec![
                TargetRoleAssignment {
                    role: edge(true),
                    targets: vec![],
                },
                TargetRoleAssignment {
                    role: edge(false),
                    targets: vec![only],
                },
            ],
        }
    });
    assert_eq!(
        chooser(&asked),
        [Viewer::P1, Viewer::P2],
        "the victim's player places it"
    );
    assert_eq!(
        ids(&game, "P2", "deck").last().map(String::as_str),
        Some("b1")
    );
    assert_eq!(
        ids(&game, "P2", "field"),
        ["b2"],
        "without Combo only the chosen one"
    );
}

#[test]
fn bp01_083_the_turn_player_looks_at_its_own_top_card_under_p1s_garden() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"garden","card":"BP01-083"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-161"},
        {"id":"top","card":"SD02-002"},
        {"id":"rest","card":"BP01-161"}
    ]);
    let mut game = loaded(&initial);
    end_phase(&mut game);
    let asked = settle(&mut game, &mut |board, viewer, domain| {
        assert_eq!(viewer, Viewer::P2, "the turn player decides");
        assert_eq!(domain.members.len(), 1, "its own top card");
        assert!(
            !sees(board, Viewer::P1, "SD02-002"),
            "the garden's controller does not see it"
        );
        Answer::Targets(vec![handle(domain, "SD02-002")])
    });
    assert_eq!(chooser(&asked), [Viewer::P2]);
    assert!(matches!(
        game.input(Viewer::P2),
        Input::Question(question) if matches!(question.shape, QuestionShape::ActionWindow(_))
    ));
    assert_eq!(ids(&game, "P2", "field"), ["top"]);
    assert_eq!(ids(&game, "P2", "hand"), ["drawn"]);
    assert_eq!(ids(&game, "P2", "deck"), ["rest"]);
    assert_eq!(ids(&game, "P1", "field"), ["garden"]);
}

#[test]
fn bp06_121_the_attacking_followers_player_pays_to_destroy_p1s_gate() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"gate","card":"BP06-121"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-161"}]);
    let mut game = loaded(&initial);
    end_phase(&mut game);
    settle(&mut game, &mut |_, viewer, domain| {
        panic!("nothing to target before the attack: {viewer:?} {domain:?}")
    });
    act(&mut game, Viewer::P2, &|kind| match kind {
        MainIntentKind::Attack { targets, .. } => {
            Some(MainActionAnswer::Attack(targets[0].clone()))
        }
        _ => None,
    });
    let asked = settle(&mut game, &mut |_, viewer, domain| {
        panic!("the gate targets nothing: {viewer:?} {domain:?}")
    });
    assert!(
        asked.contains(&(Viewer::P2, "payment")),
        "the attacking follower's player decides: {asked:?}"
    );
    assert!(!asked.contains(&(Viewer::P1, "payment")), "{asked:?}");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["pp"]["current"], 8_u8, "P2 paid two");
    assert_eq!(
        state["P1"]["pp"]["current"], 10_u8,
        "the gate's controller paid nothing"
    );
    assert_eq!(
        ids(&game, "P1", "cemetery"),
        ["gate"],
        "paid, so the gate is destroyed"
    );
    assert_eq!(
        ids(&game, "P2", "field"),
        ["attacker"],
        "paid, so no damage"
    );
}
