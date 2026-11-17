use super::declarations::{declaration_actor, with_declarations};
use super::hand_exchange_declarations::{apply, seat};
use super::target_declarations::target_program;
use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::Input;
use sve_engine::game::player::observation::{ObservedZone, ProcessingSource, TextProvenance};

fn other(actor: Viewer) -> Viewer {
    if actor == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    }
}

fn sources(game: &Game, viewer: Viewer) -> Vec<ProcessingSource> {
    let before = exact_authority(game);
    let sources = game.observe(viewer).unwrap().processing_sources;
    assert!(
        game.observe(viewer).unwrap().processing_sources == sources,
        "repeated observations retain the same disclosed source subset"
    );
    assert!(
        exact_authority(game) == before,
        "source observation is pure"
    );
    sources
}

fn empty(game: &Game, viewer: Viewer) {
    assert!(
        sources(game, viewer).is_empty(),
        "this viewer has no disclosed formal source"
    );
}

fn activate(game: &mut Game, program: &WellFormedCardProgram, actor: Viewer) {
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        actor.player(),
    )
    .unwrap();
}

#[test]
fn a_real_formal_question_discloses_its_source_to_the_answering_seat() {
    let program = target_program();
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
                    "the real main window probes declarations"
                );
                empty(&game, actor);
                empty(&game, other(actor));
            }
            assert!(
                exact_authority(&game) == before,
                "probe waits never publish a source witness"
            );
            activate(&mut game, &program, actor);
            let saved = sources(&game, actor);
            assert_eq!(
                saved.len(),
                1,
                "the actual formal question and authorized wording establish a source witness"
            );
            assert_eq!(
                saved[0].controller, actor,
                "the source records the real controller"
            );
            assert_eq!(saved[0].source.card, "unit-follower");
            assert!(
                saved[0].source.holder.is_some(),
                "the field source remains identifiable"
            );
            let waiting = game.observe(other(actor)).unwrap();
            assert!(
                waiting
                    .players
                    .iter()
                    .any(|player| player.zones.iter().any(|zone| zone
                        .cards
                        .iter()
                        .any(|card| card.card.physical.card == "unit-follower"
                            && card.card.properties.controller == actor))),
                "the waiting seat knows the public holder"
            );
            empty(&game, other(actor));
            let q = question(&game, actor);
            let before = exact_authority(&game);
            for (who, answer) in [
                (other(actor), Answer::Targets(vec![0])),
                (actor, Answer::Integer(Nat(0))),
                (actor, Answer::Targets(vec![99])),
            ] {
                assert_eq!(submit(&mut game, who, &q, answer), SubmissionKind::Rejected);
            }
            assert!(
                exact_authority(&game) == before,
                "rejection cannot grant a new audience"
            );
            assert!(
                sources(&game, actor) == saved,
                "rejection preserves the original witness"
            );
            empty(&game, other(actor));
            closed_wire(&game, actor);
            apply(&mut game, actor, Answer::Targets(vec![0]));
            assert!(
                sources(&game, actor) == saved,
                "continuation preserves the frame's witness"
            );
            finish_may(&mut game);
            empty(&game, actor);
            empty(&game, other(actor));
        }
    }
}

#[test]
fn a_frozen_source_does_not_follow_a_cost_movement_to_a_new_generation() {
    let program = program(
        "activate",
        json!({"cost":{"steps":[{"pp":1},{"self_to_cemetery":true}]},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        activate(&mut game, &program, actor);
        let before = sources(&game, actor);
        assert_eq!(before.len(), 1, "the source is disclosed before payment");
        assert!(
            before[0].source.holder.is_some(),
            "the original generation is still live"
        );
        apply(&mut game, actor, Answer::Targets(vec![0]));
        let after = sources(&game, actor);
        let mut expected = before;
        expected[0].source.holder = None;
        assert!(
            after == expected,
            "historical wording persists without reconnecting its holder"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[seat(actor)]["cemetery"][0],
            "source",
            "the cost really moved the holder"
        );
        empty(&game, other(actor));
        finish_may(&mut game);
        empty(&game, actor);
    }
}

#[test]
fn the_question_actor_is_authorized_independently_of_the_execution_controller() {
    let program = program(
        "auto",
        json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[
            {"pay":{"who":"opp","cost":{"pp":1},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}}
        ]}),
    );
    for controller in [Viewer::P1, Viewer::P2] {
        let payer = other(controller);
        let mut game = fixture(&program, controller);
        end_trigger(&mut game, controller);
        let first = sources(&game, payer);
        assert_eq!(
            first.len(),
            1,
            "the real foreign payment question establishes its audience"
        );
        assert_eq!(first[0].controller, controller);
        empty(&game, controller);
        apply(&mut game, payer, Answer::OptionalCost(true));
        assert!(
            sources(&game, payer) == first,
            "the foreign witness persists in this execution"
        );
        let second = sources(&game, controller);
        assert_eq!(
            second.len(),
            1,
            "the later actual May question discloses to its controller"
        );
        assert_eq!(second[0].controller, controller);
        finish_may(&mut game);
        empty(&game, controller);
        empty(&game, payer);
    }
}

#[test]
fn a_child_asked_only_to_the_other_seat_does_not_change_the_disclosed_parent() {
    use super::super::nested_play::{self, Scenario};
    let (mut game, definition) = nested_play::build_custom(
        "self",
        "unit-spell",
        true,
        Scenario::ProcessingPause { child_fails: false },
        |programs| {
            let mut child = programs["unit-spell"].card().clone();
            child.abilities[0]
                .node
                .play
                .as_mut()
                .unwrap()
                .parameters
                .clear();
            let body = child.abilities[0].node.body.as_mut().unwrap();
            body.steps = vec![body.steps.last().unwrap().clone()];
            let child = WellFormedCardProgram::check(child).unwrap();
            programs.insert(
                "unit-spell".into(),
                with_declarations(&child, &[declaration_actor(true)], false),
            );
        },
    );
    game.activate_internal_card("parent", definition, Viewer::P1.player())
        .unwrap();
    let parent = sources(&game, Viewer::P1);
    assert_eq!(
        parent.len(),
        1,
        "the actual outer target discloses its source"
    );
    assert_eq!(parent[0].source.card, "unit-amulet");
    empty(&game, Viewer::P2);
    apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
    let child = question(&game, Viewer::P2);
    let disclosed = sources(&game, Viewer::P2);
    assert_eq!(
        disclosed.len(),
        1,
        "the inner question discloses only its own source to P2"
    );
    assert_eq!(disclosed[0].source.card, "unit-spell");
    assert_eq!(disclosed[0].controller, Viewer::P1);
    assert!(
        sources(&game, Viewer::P1) == parent,
        "an undisclosed inner frame adds no slot or progress to its known parent"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        assert!(
            game.observe(viewer).unwrap().players[0]
                .zones
                .iter()
                .any(|zone| zone.kind == ObservedZone::Resolution
                    && zone
                        .cards
                        .iter()
                        .any(|card| card.card.physical.card == "unit-spell" && card.card.public)),
            "the child is genuinely visible to both seats in resolution"
        );
    }
    let before = exact_authority(&game);
    assert_eq!(
        submit(&mut game, Viewer::P1, &child, Answer::Integer(Nat(0))),
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "a wrong actor cannot acquire the inner witness"
    );
    apply(&mut game, Viewer::P2, Answer::Integer(Nat(0)));
    assert!(
        sources(&game, Viewer::P1) == parent,
        "return restores the same disclosed outer source"
    );
    empty(&game, Viewer::P2);
    finish_may(&mut game);
    empty(&game, Viewer::P1);
}

#[test]
fn completed_conceded_and_faulted_executions_retire_their_witnesses_but_clones_keep_their_own() {
    let program = lifecycle::with_overflow(&target_program());
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        activate(&mut game, &program, actor);
        apply(&mut game, actor, Answer::Targets(vec![0]));
        let saved = sources(&game, actor);
        assert_eq!(
            saved.len(),
            1,
            "the original has an active disclosed source"
        );
        let mut completed = game.clone();
        let mut conceded = game.clone();
        finish_may(&mut completed);
        empty(&completed, actor);
        assert!(
            sources(&game, actor) == saved,
            "completing a clone does not retire the original"
        );
        assert_eq!(
            conceded.submit_player(other(actor), Command::Concede).kind,
            SubmissionKind::Applied
        );
        empty(&conceded, actor);
        empty(&conceded, other(actor));
        assert!(
            sources(&game, actor) == saved,
            "conceding a clone does not retire the original"
        );
        let q = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        empty(&game, actor);
        empty(&game, other(actor));
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat(actor)]["pp"]["current"],
            3,
            "body fault restores payment together with source lifetime"
        );
    }
}

#[test]
fn a_raw_procedure_question_does_not_claim_a_formal_execution_source() {
    let program = program(
        "activate",
        json!({"cost":{"pp":0},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    let ability = &program.card().abilities[0];
    let body = WellFormedProgram::check(ability.node.body.clone().unwrap()).unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        let progress = game
            .execute_internal(
                &body,
                &ability.source,
                Context {
                    source: "source".into(),
                    generation: Generation(0),
                    controller: actor.player(),
                },
            )
            .unwrap();
        assert!(
            !progress.complete,
            "the raw procedure really asks a question"
        );
        empty(&game, actor);
        empty(&game, other(actor));
    }
}

fn granted_declaration(number: &str, foreign: bool) -> WellFormedCardProgram {
    let mut raw = source_provenance::granting(number, true).card().clone();
    let nested = if let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        && let Action::Grant { abilities, .. } = &mut instruction.action
        && let GrantedAbility::Definition { ability } = &mut abilities[0]
    {
        Some(ability)
    } else {
        None
    };
    assert!(
        nested.is_some(),
        "the fixture contains the original granted definition"
    );
    let nested = nested.unwrap();
    let body = nested.body.as_mut().unwrap();
    let step = &mut body.steps[0];
    let changed = if let Node::Instruction { instruction } = &mut step.node {
        instruction.action = Action::Declare {
            actor: declaration_actor(foreign),
            declaration: Declaration::NonnegativeInteger {
                binding: Binding::new(1000, body.scope),
            },
        };
        true
    } else {
        false
    };
    assert!(changed, "the real granted attack body now asks a number");
    WellFormedCardProgram::check(raw).unwrap()
}

#[test]
fn a_public_holder_cannot_authorize_an_unknown_private_wording_owner_even_for_its_question_actor() {
    use super::super::source_provenance::{attack, prepared_program};
    for actor in [Viewer::P1, Viewer::P2] {
        for foreign in [false, true] {
            let mut waiting = Vec::new();
            for number in ["unit-follower", "unit-amulet"] {
                let program = granted_declaration(number, foreign);
                let mut game = prepared_program(actor.player().seat, number, true, &program);
                attack(&mut game, actor.player().seat, "recipient");
                let pending = game.internal_pending_choice().unwrap().unwrap();
                let choices = if let InputDomain::Pending(choices) = &pending.domain {
                    Some(choices)
                } else {
                    None
                };
                assert!(choices.is_some(), "the actual granted ability is pending");
                game.answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(choices.unwrap()[0].handle),
                )
                .unwrap();
                let answerer = if foreign { other(actor) } else { actor };
                let q = question(&game, answerer);
                assert_eq!(
                    q.actor, answerer,
                    "the granted ability really asks this seat"
                );
                empty(&game, other(answerer));
                let source = sources(&game, answerer);
                if foreign {
                    assert!(
                        source.is_empty(),
                        "an actual question still requires independent wording-owner authority"
                    );
                } else {
                    assert_eq!(
                        source.len(),
                        1,
                        "the private owner is known to its own answering seat"
                    );
                    assert_eq!(source[0].source.card, number);
                    assert!(
                        matches!(&source[0].source.provenance, TextProvenance::Granted { by } if by.card == number),
                        "self-printing and cross-printing grants retain their giver"
                    );
                }
                waiting.push(game.observe(other(actor)).unwrap());
                apply(&mut game, answerer, Answer::Integer(Nat(0)));
                empty(&game, actor);
                empty(&game, other(actor));
            }
            assert!(
                waiting[0] == waiting[1],
                "the entire other-seat observation ignores which private card granted the public holder's wording"
            );
        }
    }
}

#[test]
fn nested_sources_keep_outer_to_inner_order_and_cancelled_children_add_no_witness() {
    use super::super::nested_play::{self, Scenario};
    for child_fails in [false, true] {
        let (mut game, definition) = nested_play::build(
            "self",
            "unit-spell",
            true,
            Scenario::ProcessingPause { child_fails },
        );
        game.activate_internal_card("parent", definition, Viewer::P1.player())
            .unwrap();
        let parent = sources(&game, Viewer::P1);
        assert_eq!(parent.len(), 1, "the outer source is initially disclosed");
        apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
        if !child_fails {
            let nested = sources(&game, Viewer::P1);
            assert_eq!(
                nested.len(),
                2,
                "both actual questions were disclosed to this seat"
            );
            assert!(
                nested[0] == parent[0],
                "the first disclosed source remains the outer one"
            );
            assert_eq!(
                nested[1].source.card, "unit-spell",
                "the child follows its parent"
            );
            apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
        }
        assert!(
            sources(&game, Viewer::P1) == parent,
            "a completed or cancelled child leaves only the known parent"
        );
        empty(&game, Viewer::P2);
        finish_may(&mut game);
        empty(&game, Viewer::P1);
    }
}
