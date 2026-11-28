use super::*;
use crate::player_pipeline::{answer, end_command, question};
use sve_engine::game::player::{
    ClosingAnswer, ClosingQuestion, Command, DecisionShape, DiscardKind, Input, QuestionShape,
    SubmissionKind, Viewer,
};

fn end_turn(game: &mut Game, actor: Viewer, opponent: Viewer) {
    let command = end_command(&question(game, actor));
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "the active player can enter the end phase"
    );
    if let Input::Question(current) = game.input(opponent)
        && matches!(
            current.shape,
            QuestionShape::Decision(DecisionShape::Quick { .. })
        )
    {
        assert_eq!(
            answer(game, opponent, Answer::Quick(None)).kind,
            SubmissionKind::Applied,
            "passing Quick reaches the hand limit check"
        );
    }
}

#[test]
fn player_hand_limit_is_removed_for_the_rest_of_the_game_even_after_the_source_leaves() {
    let input = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"player_effect":{"who":"self","effect":{"hand_limit":"unlimited"},"until":"game"}},
            {"discard":{"who":"self","n":1}},
            {"move":{"ref":"this","to":"cemetery"}}
        ]}),
    )]);
    let programs = lowerer().lower(&input).unwrap();
    let mut initial = setup(1, json!([]));
    for (seat, cards) in [("P1", 9_u8), ("P2", 8_u8)] {
        initial["players"][seat]["zones"]["hand"] = json!(
            (0..cards)
                .map(|index| json!({"id":format!("{seat}-hand-{index}"),"card":"unit-amulet"}))
                .collect::<Vec<_>>()
        );
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-draw-first"),"card":"unit-amulet"},
            {"id":format!("{seat}-draw-second"),"card":"unit-amulet"}
        ]);
        initial["players"][seat]["zones"]["field"] = json!([]);
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = evolution_linked_movement::loaded(&initial, &programs["unit-follower"]);
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.advance_player_input();

    let command = player_declarations::activation_command(&game, "unit-follower", 0);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied
    );
    let discard = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Discard {
        kind: DiscardKind::Effect,
        cards,
    }) = discard.shape
    else {
        panic!("the installed rule still allows effect discards");
    };
    assert_eq!(cards.cardinality.minimum, Nat(1));
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Targets(vec![cards.members[0].handle])
        )
        .kind,
        SubmissionKind::Applied
    );
    let after_ability = game.projection(View::Referee).unwrap();
    assert_eq!(
        after_ability["P1"]["cemetery"],
        json!(["P1-hand-0", "source"])
    );
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    let after_end = game.projection(View::Referee).unwrap();
    assert_eq!(after_end["turn"]["active"], "P2");
    assert_eq!(after_end["P1"]["hand"].as_array().unwrap().len(), 8);

    end_turn(&mut game, Viewer::P2, Viewer::P1);
    let closing = question(&game, Viewer::P2);
    let QuestionShape::Closing(ClosingQuestion::Discard { cards, count }) = closing.shape else {
        panic!("opponent retains the usual hand limit");
    };
    assert_eq!(count, 2);
    let before_discard = game.projection(View::Referee).unwrap();
    let legal = before_discard["legal"].as_array().unwrap();
    assert_eq!(legal.len(), 36);
    assert!(legal.iter().all(|choice| {
        choice["do"] == "end-discard"
            && choice["select"]
                .as_array()
                .is_some_and(|cards| cards.len() == 2)
    }));
    assert_eq!(
        game.submit_player(
            Viewer::P2,
            Command::Closing {
                token: closing.token,
                answer: ClosingAnswer::Discard(
                    cards
                        .iter()
                        .take(2)
                        .map(|card| card.handle.clone())
                        .collect()
                ),
            }
        )
        .kind,
        SubmissionKind::Applied
    );

    end_turn(&mut game, Viewer::P1, Viewer::P2);
    let later = game.projection(View::Referee).unwrap();
    assert_eq!(later["turn"]["active"], "P2");
    assert_eq!(later["P1"]["hand"].as_array().unwrap().len(), 9);
    assert_eq!(later["P1"]["cemetery"], after_ability["P1"]["cemetery"]);
}

#[test]
fn player_hand_limit_is_installed_by_a_printed_fanfare() {
    let input = evolution_linked_movement::tree(&[(
        "fanfare",
        json!({"do":[
            {"player_effect":{"who":"self","effect":{"hand_limit":"unlimited"},"until":"game"}}
        ]}),
    )]);
    let programs = lowerer().lower(&input).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!(
        (0_u8..9)
            .map(|index| {
                let number = if index == 0 {
                    "unit-follower"
                } else {
                    "unit-amulet"
                };
                json!({"id":format!("hand-{index}"),"card":number})
            })
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"unit-amulet"}]);
    let mut game = evolution_linked_movement::loaded(&initial, &programs["unit-follower"]);
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    let command = player_declarations::play_command(&game, "unit-follower");
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied
    );
    if let Input::Question(current) = game.input(Viewer::P1)
        && let QuestionShape::Decision(DecisionShape::Pending(choices)) = current.shape
    {
        assert_eq!(
            answer(&mut game, Viewer::P1, Answer::Pending(choices[0].handle)).kind,
            SubmissionKind::Applied
        );
    }
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 8);
}

#[test]
fn player_hand_limit_rejects_other_recipients_and_temporary_durations() {
    for (who, duration) in [("self", "turn"), ("opp", "game")] {
        let input = evolution_linked_movement::tree(&[(
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"player_effect":{"who":who,"effect":{"hand_limit":"unlimited"},"until":duration}}
            ]}),
        )]);
        lowerer().lower(&input).unwrap_err();
    }
}

#[test]
fn player_hand_limit_rejects_changed_actor_lifetime_recipient_and_source_in_typed_ir() {
    let input = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"player_effect":{"who":"self","effect":{"hand_limit":"unlimited"},"until":"game"}}
        ]}),
    )]);
    let programs = lowerer().lower(&input).unwrap();
    assert_player_rule_rejects_mutations(&programs["unit-follower"]);
}

pub(super) fn assert_player_rule_rejects_mutations(program: &WellFormedCardProgram) {
    #[derive(Debug)]
    enum Mutation {
        Actor,
        Duration,
        Recipient,
        Source,
    }

    for variant in [
        Mutation::Actor,
        Mutation::Duration,
        Mutation::Recipient,
        Mutation::Source,
    ] {
        let mut raw = program.card().clone();
        let body = raw.abilities[0].node.body.as_mut().unwrap();
        let Node::Instruction { instruction } = &mut body.steps[0].node else {
            panic!("player rule instruction");
        };
        let Action::InstallEffect {
            subjects: InstallationSubjects::Players { players, .. },
            effect,
            duration,
            ..
        } = &mut instruction.action
        else {
            panic!("player rule installation");
        };
        let Effect::RuleModifier { source, .. } = effect.as_mut() else {
            panic!("player rule modifier");
        };
        match variant {
            Mutation::Actor => instruction.actor = PlayerRef::Active {},
            Mutation::Duration => duration.end = DurationEnd::CurrentTurnEnd {},
            Mutation::Recipient => players.relation = PlayerRelation::Opponents,
            Mutation::Source => {
                let printed = match &mut source.origin {
                    AbilityOrigin::Keyword { origin, .. } => origin.as_mut(),
                    printed @ AbilityOrigin::Printed { .. } => printed,
                    AbilityOrigin::Granted { .. } => panic!("expected printed source"),
                };
                let AbilityOrigin::Printed { line, .. } = printed else {
                    panic!("expected a printed line");
                };
                *line = Nat(99);
            }
        }
        assert!(WellFormedCardProgram::check(raw).is_err(), "{variant:?}");
    }
}
