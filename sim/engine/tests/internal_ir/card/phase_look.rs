//! BP01-083's shape: whenever a player's main phase begins, that player looks at its own deck
//! top and may put a follower from it onto its field. The ability's controller stays the
//! amulet's controller (Q204).
#![expect(
    clippy::panic,
    reason = "A missing look question invalidates the scenario."
)]
use super::evolution_linked_movement::tree;
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::game::player::{Input, Viewer};

fn programs() -> BTreeMap<String, WellFormedCardProgram> {
    let mut tree = tree(&[(
        "auto",
        json!({"on":{"phase":{"which":"main","whose":"turn"}},"do":[
            {"look":{"top":{"who":"turn","n":1},"bind":"seen","by":"turn"}},
            {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"upto":1,"by":"turn"}},
            {"move":{"ref":"picked","to":"field","result":"entered"}}
        ]}),
    )]);
    tree["cards"]["unit-amulet"] = tree["cards"]["unit-follower"].clone();
    tree["cards"]
        .as_object_mut()
        .unwrap()
        .remove("unit-follower");
    lowerer().lower(&tree).unwrap()
}

/// P1 controls the amulet; `next` is whose main phase begins after the current turn ends.
/// Each deck draws `x` at its turn start, leaving the follower `f` on top.
fn main_begins(next: &str) -> (Game, DecisionView) {
    let mut game = begin(next, false);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(
        pending.actor,
        PlayerId { seat: 1 },
        "the amulet's controller plays it"
    );
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the main phase trigger is pending");
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("the main phase player decides");
    };
    (game, question)
}

/// Ends the current turn so that `next`'s main phase begins; with `both`, P2 has a garden too.
fn begin(next: &str, both: bool) -> Game {
    let programs = programs();
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!(if next == "P1" { "P2" } else { "P1" });
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"garden","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = if both {
        json!([{"id":"rival","card":"unit-amulet"}])
    } else {
        json!([])
    };
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-x"),"card":"unit-spell"},
            {"id":format!("{seat}-f"),"card":"unit-follower"},
            {"id":format!("{seat}-y"),"card":"unit-spell"}
        ]);
    }
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "garden").unwrap();
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game
}

fn ids(game: &Game, player: &str, zone: &str) -> Vec<String> {
    game.projection(View::Referee).unwrap()[player][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

fn members(question: &DecisionView) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a looked card question");
    };
    domain.members.iter().map(|member| member.handle).collect()
}

#[test]
fn the_main_phase_player_looks_at_its_own_top_and_may_put_a_follower() {
    for (next, seat) in [("P1", 1_u32), ("P2", 2)] {
        let (mut game, question) = main_begins(next);
        assert_eq!(question.actor, PlayerId { seat }, "{next} decides");
        assert_eq!(members(&question).len(), 1, "{next} sees its own top card");
        let other = if seat == 1 { Viewer::P2 } else { Viewer::P1 };
        assert!(matches!(game.input(other), Input::Waiting(_)), "{next}");
        let hidden = format!("\"{next}-f\"");
        let other_view = if seat == 1 { View::P2 } else { View::P1 };
        assert!(
            !game
                .projection(other_view)
                .unwrap()
                .to_string()
                .contains(&hidden),
            "the other player does not learn the looked card"
        );
        let before = exact_authority(&game);
        game.answer_internal_card(
            PlayerId {
                seat: if seat == 1 { 2 } else { 1 },
            },
            question.token,
            Answer::Targets(members(&question)),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before, "only {next} answers");
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(members(&question)),
            )
            .unwrap(),
        );
        assert!(
            ids(&game, next, "field").contains(&format!("{next}-f")),
            "the follower enters {next}'s own field"
        );
    }
}

#[test]
fn declining_leaves_the_looked_card_on_top() {
    let (mut game, question) = main_begins("P2");
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    assert_eq!(ids(&game, "P2", "deck")[0], "P2-f");
    assert_eq!(ids(&game, "P2", "field"), Vec::<String>::new());
}

#[test]
fn the_turn_players_own_garden_resolves_first() {
    // Q204: on P2's main phase, P2's garden resolves before P1's, and in both P2 looks.
    let mut game = begin("P2", true);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(
        pending.actor,
        PlayerId { seat: 2 },
        "the turn player's trigger first"
    );
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the turn player's trigger is pending");
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("P2 looks at its own top");
    };
    assert_eq!(question.actor, PlayerId { seat: 2 });
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(members(&question)),
        )
        .unwrap(),
    );
    assert!(ids(&game, "P2", "field").contains(&"P2-f".to_owned()));
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming)
        && game.internal_pending_choice().is_err()
    {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(pending.actor, PlayerId { seat: 1 }, "then P1's garden");
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("P1's trigger is pending");
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("P2 looks again");
    };
    assert_eq!(
        question.actor,
        PlayerId { seat: 2 },
        "P1's garden also has the turn player P2 look at its own deck"
    );
}

/// P1 activates its amulet whose body is `steps`; each deck has its own follower on top, and
/// P2's field holds `rival_field`.
fn activated(steps: &Value, rival_field: &Value) -> (Game, Result<CardPlay, EngineFailure>) {
    let mut tree = tree(&[("activate", json!({"cost":{"pp":0},"do":steps}))]);
    tree["cards"]["unit-amulet"] = tree["cards"]["unit-follower"].clone();
    tree["cards"]
        .as_object_mut()
        .unwrap()
        .remove("unit-follower");
    let programs = lowerer().lower(&tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"garden","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = rival_field.clone();
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-f"),"card":"unit-follower"},
            {"id":format!("{seat}-y"),"card":"unit-spell"}
        ]);
    }
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "looks").unwrap();
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let id = programs["unit-amulet"].card().abilities[0].node.id;
    let play = game.activate_internal_card("garden", id, PlayerId { seat: 1 });
    (game, play)
}

#[test]
fn each_player_chooses_from_its_own_look_in_a_later_stage() {
    // Both players look first; each then chooses from its own looked card, P1 first.
    let (mut game, play) = activated(
        &json!([{"each_player":{"as":"p","stages":[
            [{"look":{"top":{"who":"p","n":1},"bind":"seen","by":"p"}}],
            [{"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"upto":1,"by":"p"}},
             {"move":{"ref":"picked","to":"field","result":"entered"}}]
        ]}}]),
        &json!([]),
    );
    let mut play = play.unwrap();
    for (seat, own, other) in [(1_u32, "P1-f", View::P2), (2, "P2-f", View::P1)] {
        let CardPlay::Awaiting(question) = play else {
            panic!("player {seat} chooses from its own look");
        };
        assert_eq!(question.actor, PlayerId { seat });
        assert_eq!(members(&question).len(), 1, "player {seat}'s own top card");
        assert!(
            !game
                .projection(other)
                .unwrap()
                .to_string()
                .contains(&format!("\"{own}\"")),
            "the other player does not learn {own}"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(members(&question)),
            )
            .unwrap();
    }
    resolved(play);
    assert!(ids(&game, "P1", "field").contains(&"P1-f".to_owned()));
    assert!(ids(&game, "P2", "field").contains(&"P2-f".to_owned()));
}

#[test]
fn an_omitted_look_viewer_is_the_effect_controller() {
    // `by` defaults to self: omitting it or naming self looks as the effect controller.
    for by in [None, Some("self")] {
        let mut look = json!({"top":{"who":"turn","n":1},"bind":"seen"});
        let mut own = json!({"top":{"who":"self","n":1},"bind":"seen"});
        if let Some(by) = by {
            look["by"] = json!(by);
            own["by"] = json!(by);
        }
        let lowered = |look: &Value| {
            let mut tree = tree(&[("activate", json!({"cost":{"pp":0},"do":[{"look":look}]}))]);
            tree["cards"]["unit-amulet"] = tree["cards"]["unit-follower"].clone();
            lowerer()
                .lower(&tree)
                .map(|_| ())
                .map_err(|error| error.code)
        };
        assert_eq!(
            lowered(&look),
            Err("unsupported_construct"),
            "the effect controller cannot privately look at the turn player's deck: {by:?}"
        );
        assert_eq!(lowered(&own), Ok(()), "its own deck: {by:?}");
    }
}

#[test]
fn a_shuffle_revokes_a_saved_look_even_after_a_new_look() {
    // P2's stage shuffles P1's deck and, optionally, P1 looks again under a new name; P1's
    // saved look from before the shuffle no longer offers its card either way.
    for fresh in [false, true] {
        let mut then = vec![json!({"shuffle":{"who":"self"}})];
        if fresh {
            then.push(json!({"look":{"top":{"who":"self","n":2},"bind":"fresh","by":"self"}}));
        }
        let (mut game, play) = activated(
            &json!([{"each_player":{"as":"p","stages":[
                [{"look":{"top":{"who":"p","n":1},"bind":"seen","by":"p"}},
                 {"if":{"cond":{"cmp":{"left":{"count":{"who":"p","zone":"field","kind":"follower"}},"op":"gt","right":0}},
                    "then":then}}],
                [{"if":{"cond":{"cmp":{"left":{"count":{"who":"p","zone":"field","kind":"follower"}},"op":"eq","right":0}},
                    "then":[
                        {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"upto":1,"by":"p"}},
                        {"move":{"ref":"picked","to":"field","result":"entered"}}
                    ]}}]
            ]}}]),
            &json!([{"id":"marker","card":"unit-follower"}]),
        );
        match play {
            Ok(CardPlay::Awaiting(question)) => {
                assert_eq!(members(&question), Vec::<u64>::new(), "fresh={fresh}");
                resolved(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(vec![]),
                    )
                    .unwrap(),
                );
            }
            Ok(other) => {
                resolved(other);
            }
            Err(failure) => {
                assert!(
                    matches!(failure, EngineFailure::Unsupported(_)),
                    "fresh={fresh}: {failure:?}"
                );
            }
        }
        assert!(
            !ids(&game, "P1", "field").contains(&"P1-f".to_owned()),
            "fresh={fresh}: the revoked look never puts P1-f on the field"
        );
    }
}

#[test]
fn the_turn_player_may_answer_a_public_choice() {
    // P1's amulet triggers on P2's main phase; P2, the turn player, picks one of P1's followers.
    let mut tree = tree(&[(
        "auto",
        json!({"on":{"phase":{"which":"main","whose":"opp"}},"do":[
            {"choose":{"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1,"by":"turn"}},
            {"destroy":"t"}
        ]}),
    )]);
    tree["cards"]["unit-amulet"] = tree["cards"]["unit-follower"].clone();
    tree["cards"]
        .as_object_mut()
        .unwrap()
        .remove("unit-follower");
    let programs = lowerer().lower(&tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P1");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"garden","card":"unit-amulet"},
        {"id":"m0","card":"unit-follower"},
        {"id":"m1","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-x"),"card":"unit-spell"},
            {"id":format!("{seat}-y"),"card":"unit-spell"}
        ]);
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "turn-choice",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the main phase trigger is pending");
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("the turn player chooses");
    };
    assert_eq!(
        question.actor,
        PlayerId { seat: 2 },
        "the turn player answers"
    );
    assert_eq!(members(&question).len(), 2);
    let before = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 1 },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        before,
        "only the turn player answers"
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        ids(&game, "P1", "field").len(),
        2,
        "one follower was destroyed"
    );
    assert_eq!(ids(&game, "P1", "cemetery").len(), 1);
}
