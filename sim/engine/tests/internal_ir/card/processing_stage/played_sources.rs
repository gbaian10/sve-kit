use super::declarations::{declaration_actor, with_declarations};
use super::frame_sources::{activate, empty, other, sources};
use super::hand_exchange_declarations::{apply, hand_program, seat, spell_game, spell_program};
use super::target_declarations::target_program;
use super::*;
use core::iter::repeat_with;
use sve_engine::game::player::{DecisionShape, Input};

#[test]
fn a_public_abilitys_played_writer_discloses_its_source_to_the_waiting_seat() {
    let program = target_program();
    for actor in [Viewer::P1, Viewer::P2] {
        for journal in [false, true] {
            let mut game = fixture(&program, actor);
            if journal {
                game.advance_player_input();
            }
            activate(&mut game, &program, actor);
            empty(&game, other(actor));
            let q = game.internal_decision(View::Referee).unwrap();
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
                .unwrap();
            assert_eq!(
                game.internal_play_phase(),
                Some(PlayPhase::Resolve),
                "the real played writer succeeded before its body May"
            );
            let seen = sources(&game, other(actor));
            assert_eq!(
                seen.len(),
                1,
                "the actual public played writer authorizes a waiting audience without asking it"
            );
            assert_eq!(seen[0].controller, actor);
            assert_eq!(seen[0].source.card, "unit-follower");
            let result = finish_may(&mut game);
            assert_eq!(
                result
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                    .count(),
                1,
                "the real played writer recorded exactly one ability play"
            );
            empty(&game, actor);
            empty(&game, other(actor));
        }
    }
}

#[test]
fn public_plays_without_a_parameter_question_do_not_depend_on_journal_or_probe_waits() {
    let program = program(
        "activate",
        json!({"cost":{"pp":0},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        for journal in [false, true] {
            let mut game = fixture(&program, actor);
            if journal {
                game.advance_player_input();
            }
            let before = exact_authority(&game);
            for _ in 0..2 {
                assert!(
                    matches!(game.input(actor), Input::Question(_)),
                    "the ordinary main window runs its real availability probes"
                );
                empty(&game, actor);
                empty(&game, other(actor));
            }
            assert!(
                exact_authority(&game) == before,
                "previewing an available public play grants no witness"
            );
            activate(&mut game, &program, actor);
            for viewer in [actor, other(actor)] {
                let seen = sources(&game, viewer);
                assert_eq!(
                    seen.len(),
                    1,
                    "the played writer discloses even without a preceding formal question"
                );
                assert_eq!(seen[0].controller, actor);
            }
            assert!(
                matches!(game.input(other(actor)), Input::Waiting(_)),
                "the other audience was never asked"
            );
            closed_wire(&game, actor);
            finish_may(&mut game);
        }
    }
}

#[test]
fn a_private_holder_still_does_not_disclose_a_source_to_the_other_seat() {
    let program = with_declarations(
        &source_provenance::granting("unit-follower", true),
        &[PlayerRef::Controller {}],
        false,
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture_with(&program, actor, |initial| {
            initial["players"][seat(actor)]["zones"]["field"] = json!([]);
            initial["players"][seat(actor)]["zones"]["hand"] =
                json!([{"id":"source","card":"unit-follower"}]);
        });
        activate(&mut game, &program, actor);
        assert_eq!(
            game.internal_play_phase(),
            Some(PlayPhase::Resolve),
            "the private holder really reached its formal played body"
        );
        assert_eq!(
            sources(&game, actor).len(),
            1,
            "the actual question authorizes its own seat"
        );
        empty(&game, other(actor));
        assert!(
            matches!(game.input(other(actor)), Input::Waiting(_)),
            "the private source adds no question or frame identity for the other seat"
        );
        apply(&mut game, actor, Answer::Integer(Nat(0)));
        empty(&game, actor);
        empty(&game, other(actor));
    }
}

#[test]
fn choosing_materials_does_not_disclose_a_play_until_formal_payment_completes() {
    let program = hand_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture_with(&program, actor, |initial| {
            initial["players"][seat(actor)]["zones"]["hand"] =
                json!([{"id":"fee","card":"unit-spell"}]);
        });
        activate(&mut game, &program, actor);
        assert_eq!(
            game.internal_play_phase(),
            Some(PlayPhase::Declare),
            "cost materials are still being declared before payment"
        );
        empty(&game, other(actor));
        let q = question(&game, actor);
        let before = exact_authority(&game);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::CostMaterials(vec![99])),
            SubmissionKind::Rejected
        );
        assert!(
            exact_authority(&game) == before,
            "invalid payment materials cannot create a played witness"
        );
        empty(&game, other(actor));
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
        assert_eq!(
            sources(&game, other(actor)).len(),
            1,
            "the successful payment reaches the real played writer"
        );
        let result = finish_may(&mut game);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .count(),
            1
        );
        empty(&game, actor);
        empty(&game, other(actor));
    }
}

#[test]
fn an_already_public_resolution_card_does_not_disclose_its_frame_before_it_is_played() {
    let program = spell_program(
        json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"exchange"},"optional_cost_delta":-2}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = spell_game(&program, actor);
        empty(&game, other(actor));
        let q = question(&game, actor);
        assert!(
            matches!(
                q.shape,
                QuestionShape::Decision(DecisionShape::OptionalCost)
            ),
            "the exchange route has not been accepted or declined"
        );
        apply(&mut game, actor, Answer::OptionalCost(false));
        let seen = sources(&game, other(actor));
        assert_eq!(
            seen.len(),
            1,
            "the Spell's played writer grants the waiting audience after intrinsic payment"
        );
        assert_eq!(seen[0].source.card, "unit-spell");
        assert!(
            seen[0].source.holder.is_some(),
            "the resolving Spell still has its authorized physical generation"
        );
        let result = finish_may(&mut game);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::CardPlayed { .. }))
                .count(),
            1
        );
        empty(&game, actor);
        empty(&game, other(actor));
    }
}

#[test]
fn already_disclosed_sources_reveal_no_private_remaining_question_count() {
    let base = program(
        "activate",
        json!({"cost":{"pp":0},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut boards = Vec::new();
        let mut waiting = Vec::new();
        for count in [2, 3] {
            let mut players = vec![PlayerRef::Controller {}];
            players.extend(repeat_with(|| declaration_actor(true)).take(count));
            let program = with_declarations(&base, &players, false);
            let mut game = fixture(&program, actor);
            activate(&mut game, &program, actor);
            assert_eq!(
                sources(&game, actor).len(),
                1,
                "the viewer first receives its own real question"
            );
            apply(&mut game, actor, Answer::Integer(Nat(0)));
            assert!(
                matches!(game.input(actor), Input::Waiting(_)),
                "both variants now wait for the other seat"
            );
            let first = game.observe(actor).unwrap();
            let first_wait = game.input(actor);
            apply(&mut game, other(actor), Answer::Integer(Nat(0)));
            assert!(
                matches!(game.input(actor), Input::Waiting(_)),
                "both variants still wait while a different number remains"
            );
            boards.push((first, game.observe(actor).unwrap()));
            waiting.push((first_wait, game.input(actor)));
        }
        assert!(
            boards[0] == boards[1],
            "the entire already-authorized native board ignores private remaining questions"
        );
        assert!(
            waiting[0] == waiting[1],
            "the entire waiting summary ignores private remaining questions"
        );
    }
}

#[test]
fn a_known_parent_does_not_expose_unplayed_child_private_candidate_counts() {
    use super::super::nested_play::{self, Scenario};
    let mut boards = Vec::new();
    let mut waiting = Vec::new();
    let mut counts = Vec::new();
    for eligible in [1, 2] {
        let (mut game, definition) = nested_play::build_position(
            "self",
            "unit-spell",
            true,
            Scenario::MandatoryReveal { material: true },
            |_| {},
            |initial| {
                initial["players"]["P1"]["zones"]["hand"] = json!((0..2).map(|index| json!({"id":format!("h{index}"),"card":if index<eligible {"unit-follower"} else {"unit-spell"}})).collect::<Vec<_>>());
            },
        );
        game.activate_internal_card("parent", definition, Viewer::P1.player())
            .unwrap();
        empty(&game, Viewer::P2);
        apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
        let seen = sources(&game, Viewer::P2);
        assert_eq!(
            seen.len(),
            1,
            "the actual parent played writer is independently disclosed"
        );
        assert_eq!(
            seen[0].source.card, "unit-amulet",
            "the unplayed child leaves no source slot"
        );
        let q = question(&game, Viewer::P1);
        let members = if let QuestionShape::Decision(DecisionShape::CostMaterials(domain)) = q.shape
        {
            Some(domain.members.len())
        } else {
            None
        };
        assert!(
            members.is_some(),
            "the child's actual private reveal material question is pending"
        );
        counts.push(members.unwrap());
        boards.push(game.observe(Viewer::P2).unwrap());
        waiting.push(game.input(Viewer::P2));
        closed_wire(&game, Viewer::P1);
    }
    assert_eq!(
        counts,
        [1, 2],
        "the real hidden candidate sets differ with equal hand sizes"
    );
    assert!(
        boards[0] == boards[1],
        "the entire known-parent native board ignores the private child candidate count"
    );
    assert!(
        waiting[0] == waiting[1],
        "the waiting summary ignores the private child candidate count"
    );
}
