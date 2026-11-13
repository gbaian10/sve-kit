#![expect(
    clippy::panic,
    reason = "A missing decision invalidates the player choice scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree(number: &str, head: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let card = &tree["cards"]["BP08-038"];
    let meta = card["meta"].clone();
    let mut entry = card["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    for (key, value) in head.as_object().unwrap() {
        entry[key] = value.clone();
    }
    tree["cards"] = json!({ number: {"meta":meta,"abilities":[entry]} });
    tree
}

/// BP06-107's shape: each player keeps one own follower and sends the rest to the cemetery.
fn keep_one() -> Value {
    tree(
        "unit-follower",
        &json!({"fanfare":{"do":[{"each_player":{"as":"p","stages":[[
            {"choose":{"bind":"kept","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}},
            {"move":{"ref":{"who":"p","zone":"field","kind":"follower","except":{"in":"kept"}},"to":"cemetery"}}
        ]]}}]}}),
    )
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("a player decision must be offered");
    };
    question
}

fn handles(question: &DecisionView) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a target question");
    };
    domain.members.iter().map(|member| member.handle).collect()
}

fn count(state: &Value, player: &str, zone: &str) -> usize {
    state[player][zone].as_array().unwrap().len()
}

#[test]
fn each_player_keeps_one_follower_after_the_turn_player_has_finished() {
    // Q866: the turn player chooses and sends the rest away before the other player chooses.
    let programs = lowerer().lower(&keep_one()).unwrap();
    for seat in [1_u32, 2] {
        let (me, them) = if seat == 1 {
            ("P1", "P2")
        } else {
            ("P2", "P1")
        };
        let other = if seat == 1 { 2 } else { 1 };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(me);
        initial["players"][me]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"][me]["pp"] = json!({"current":5,"max":5});
        initial["players"][me]["zones"]["field"] = json!([{"id":"m1","card":"unit-follower"}]);
        initial["players"][them]["zones"]["field"] = json!([
            {"id":"t1","card":"unit-follower"},
            {"id":"t2","card":"unit-follower"},
            {"id":"t3","card":"unit-follower"}
        ]);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "keep-one").unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game.play_internal_card("s", PlayerId { seat }).unwrap();
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("one pending fanfare");
        };
        let first = awaiting(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        assert_eq!(
            first.actor,
            PlayerId { seat },
            "the turn player chooses first"
        );
        assert_eq!(handles(&first).len(), 2, "its own two followers");
        let second = awaiting(
            game.answer_internal_card(
                first.actor,
                first.token,
                Answer::Targets(vec![handles(&first)[0]]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            (count(&state, me, "field"), count(&state, me, "cemetery")),
            (1, 1),
            "the turn player finished before the other player chooses"
        );
        assert_eq!(count(&state, them, "cemetery"), 0);
        assert_eq!(second.actor, PlayerId { seat: other });
        assert_eq!(handles(&second).len(), 3);
        let before = exact_authority(&game);
        game.answer_internal_card(
            PlayerId { seat },
            second.token,
            Answer::Targets(vec![handles(&second)[0]]),
        )
        .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "only the stage player answers"
        );
        resolved(
            game.answer_internal_card(
                second.actor,
                second.token,
                Answer::Targets(vec![handles(&second)[1]]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            (
                count(&state, them, "field"),
                count(&state, them, "cemetery")
            ),
            (1, 2),
            "seat={seat}"
        );
        assert_eq!(
            (count(&state, me, "field"), count(&state, me, "cemetery")),
            (1, 1)
        );
    }
}

#[test]
fn a_procedure_player_is_named_only_inside_its_procedure() {
    let choose = |who: &str, by: &str| json!({"choose":{"bind":"kept","from":{"who":who,"zone":"field","kind":"follower"},"n":1,"by":by}});
    for (steps, case) in [
        (
            json!([choose("p", "self")]),
            "who names no procedure outside it",
        ),
        (
            json!([choose("self", "p")]),
            "by names no procedure outside it",
        ),
        (
            json!([{"each_player":{"as":"p","stages":[[choose("q", "p")]]}}]),
            "an unknown player name",
        ),
    ] {
        assert_eq!(
            lowerer()
                .lower(&tree("unit-follower", &json!({"fanfare":{"do":steps}})))
                .unwrap_err()
                .code,
            "unsupported_construct",
            "{case}"
        );
    }
}

fn cards(prefix: &str, count: usize, card: &str) -> Value {
    Value::Array(
        (0..count)
            .map(|index| json!({"id":format!("{prefix}{index}"),"card":card}))
            .collect(),
    )
}

/// P1 plays `s` with `steps` as its fanfare; returns the game and the first question, if any.
fn play(
    steps: &Value,
    fields: (usize, usize),
    hands: (Value, Value),
) -> (Game, Option<DecisionView>) {
    let programs = lowerer()
        .lower(&tree("unit-follower", &json!({"fanfare":{"do":steps}})))
        .unwrap();
    let mut initial = setup(1, json!([]));
    let mut mine = vec![json!({"id":"s","card":"unit-follower"})];
    mine.extend(hands.0.as_array().unwrap().iter().cloned());
    initial["players"]["P1"]["zones"]["hand"] = Value::Array(mine);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P1"]["zones"]["field"] = cards("m", fields.0, "unit-follower");
    initial["players"]["P2"]["zones"]["field"] = cards("t", fields.1, "unit-follower");
    initial["players"]["P2"]["zones"]["hand"] = hands.1;
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "procedure").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending fanfare");
    };
    let next = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let question = if let CardPlay::Awaiting(question) = next {
        Some(question)
    } else {
        None
    };
    (game, question)
}

fn answer(game: &mut Game, question: &DecisionView, picked: Vec<u64>) -> Option<DecisionView> {
    if let CardPlay::Awaiting(next) = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(picked))
        .unwrap()
    {
        Some(next)
    } else {
        None
    }
}

fn ids(state: &Value, player: &str, zone: &str) -> Vec<String> {
    let mut ids = state[player][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[test]
fn a_later_stage_reads_each_players_own_earlier_choice() {
    // P1 keeps the second of {s, m0}; P2 keeps the first of {t0, t1}.
    let steps = json!([{"each_player":{"as":"p","stages":[
        [{"choose":{"bind":"kept","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}}],
        [{"move":{"ref":{"who":"p","zone":"field","kind":"follower","except":{"in":"kept"}},"to":"cemetery"}}]
    ]}}]);
    let (mut game, first) = play(&steps, (1, 2), (json!([]), json!([])));
    let first = first.unwrap();
    let second = answer(&mut game, &first, vec![handles(&first)[1]]).unwrap();
    assert!(
        answer(&mut game, &second, vec![handles(&second)[0]]).is_none(),
        "both choices are made before either player's later stage"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P1", "field").len(), 1);
    assert_eq!(ids(&state, "P2", "field").len(), 1);
    assert_eq!(
        ids(&state, "P1", "cemetery").len() + ids(&state, "P2", "cemetery").len(),
        2,
        "each player sent away only the follower they did not keep: {state}"
    );
}

#[test]
fn an_action_naming_the_effect_controller_keeps_that_player() {
    // Every player discards the effect controller's hand: P2's hand is untouched.
    let direct = json!([{"each_player":{"as":"p","stages":[
        [{"discard":{"all":true,"who":"self"}}]
    ]}}]);
    let wrapped = json!([{"each_player":{"as":"p","stages":[
        [{"if":{"cond":{"cmp":{"left":{"resource":{"who":"self","kind":"pp_max"}},"op":"ge","right":0}},
            "then":[{"discard":{"all":true,"who":"self"}}]}}]
    ]}}]);
    for steps in [direct, wrapped] {
        let (game, question) = play(
            &steps,
            (0, 0),
            (cards("k", 2, "unit-spell"), cards("o", 2, "unit-spell")),
        );
        assert!(question.is_none());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(ids(&state, "P1", "hand"), Vec::<String>::new(), "{steps}");
        assert_eq!(ids(&state, "P1", "cemetery"), ["k0", "k1"], "{steps}");
        assert_eq!(ids(&state, "P2", "hand"), ["o0", "o1"], "{steps}");
    }
}

#[test]
fn independent_procedures_may_reuse_a_choice_name() {
    let once = json!({"each_player":{"as":"p","stages":[[
        {"choose":{"bind":"kept","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}},
        {"move":{"ref":{"who":"p","zone":"field","kind":"follower","except":{"in":"kept"}},"to":"cemetery"}}
    ]]}});
    let (mut game, question) = play(&json!([once, once]), (1, 1), (json!([]), json!([])));
    let mut question = question;
    while let Some(current) = question {
        question = answer(&mut game, &current, vec![handles(&current)[0]]);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P1", "field").len(), 1);
    assert_eq!(ids(&state, "P2", "field").len(), 1);
}

#[test]
fn a_procedure_player_reads_its_own_hand_count_and_reveal() {
    // Each player with at least one hand card reveals it and keeps one chosen hand follower.
    let steps = json!([{"each_player":{"as":"p","stages":[[
        {"if":{"cond":{"cmp":{"left":{"count":{"who":"p","zone":"hand"}},"op":"ge","right":1}},
            "then":[{"reveal":{"bind":"shown","ref":{"who":"p","zone":"hand"}}}]}},
        {"choose":{"bind":"held","from":{"who":"p","zone":"hand","kind":"follower"},"upto":1,"by":"p"}},
        {"move":{"ref":"held","to":"field","result":"entered"}}
    ]]}}]);
    let (mut game, first) = play(
        &steps,
        (0, 0),
        (
            cards("k", 1, "unit-follower"),
            cards("o", 1, "unit-follower"),
        ),
    );
    let first = first.unwrap();
    assert_eq!(first.actor, PlayerId { seat: 1 });
    assert_eq!(
        handles(&first).len(),
        1,
        "P1 sees only its own hand follower"
    );
    let second = answer(&mut game, &first, handles(&first)).unwrap();
    assert_eq!(second.actor, PlayerId { seat: 2 });
    assert_eq!(
        handles(&second).len(),
        1,
        "P2 sees only its own hand follower"
    );
    let before = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 1 },
        second.token,
        Answer::Targets(handles(&second)),
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    assert!(answer(&mut game, &second, handles(&second)).is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P1", "field"), ["k0", "s"]);
    assert_eq!(ids(&state, "P2", "field"), ["o0"]);
}

#[test]
fn the_turn_player_chooses_first_even_when_it_is_not_the_effect_controller() {
    let programs = lowerer()
        .lower(&tree(
            "unit-amulet",
            &json!({"auto":{
                "on":{"attacked":{"who":"opp","sel":{"kind":"follower"}}},
                "do":[{"each_player":{"as":"p","stages":[[
                    {"choose":{"bind":"kept","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}},
                    {"move":{"ref":{"who":"p","zone":"field","kind":"follower","except":{"in":"kept"}},"to":"cemetery"}}
                ]]}}]
            }}),
        ))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"a","card":"unit-amulet"},
        {"id":"m0","card":"unit-follower"},
        {"id":"m1","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = cards("t", 2, "unit-follower");
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "turn-first",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    for number in ["unit-spell", "unit-follower"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.internal_rule_action(&json!({"do":"attack","attacker":"t0","target":"P1.leader"}))
        .unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending trigger");
    };
    let first = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        first.actor,
        PlayerId { seat: 2 },
        "the turn player chooses first"
    );
    let second = answer(&mut game, &first, vec![handles(&first)[0]]).unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P2", "field").len(), 1, "P2 finished first");
    assert_eq!(second.actor, PlayerId { seat: 1 });
    assert!(answer(&mut game, &second, vec![handles(&second)[0]]).is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        ids(&state, "P1", "field").len(),
        2,
        "the amulet and one kept follower"
    );
    assert_eq!(ids(&state, "P1", "cemetery").len(), 1);
}

#[test]
fn a_later_stage_reads_each_players_own_receipt_after_a_choice() {
    // P1 returns three hand cards and P2 two; each draws back exactly its own count.
    let steps = json!([{"each_player":{"as":"p","stages":[
        [
            {"choose":{"bind":"kept","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}},
            {"move":{"ref":{"who":"p","zone":"hand"},"to":"deck","result":"returned"}},
            {"shuffle":{"who":"p"}}
        ],
        [{"draw":{"who":"p","n":{"count_of":"returned"}}}]
    ]}}]);
    let programs = lowerer()
        .lower(&tree("unit-follower", &json!({"fanfare":{"do":steps}})))
        .unwrap();
    let mut initial = setup(1, json!([]));
    let mut hand = vec![json!({"id":"s","card":"unit-follower"})];
    hand.extend(
        cards("k", 3, "unit-spell")
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    initial["players"]["P1"]["zones"]["hand"] = Value::Array(hand);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P2"]["zones"]["hand"] = cards("o", 2, "unit-spell");
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["deck"] = cards(&format!("{player}d"), 3, "unit-spell");
    }
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "mixed").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending fanfare");
    };
    let mut question = Some(awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    ));
    while let Some(current) = question {
        question = answer(&mut game, &current, vec![handles(&current)[0]]);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P1", "hand").len(), 3, "{state}");
    assert_eq!(ids(&state, "P2", "hand").len(), 2, "{state}");
}

#[test]
fn a_nested_procedure_leaves_the_outer_players_choices_intact() {
    let steps = json!([{"each_player":{"as":"p","stages":[
        [{"choose":{"bind":"kept","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}}],
        [{"each_player":{"as":"q","stages":[[{"life":{"who":"self","delta":1}}]]}}],
        [{"move":{"ref":{"who":"p","zone":"field","kind":"follower","except":{"in":"kept"}},"to":"cemetery"}}]
    ]}}]);
    let (mut game, first) = play(&steps, (1, 2), (json!([]), json!([])));
    let first = first.unwrap();
    let kept_mine = handles(&first)[1];
    let second = answer(&mut game, &first, vec![kept_mine]).unwrap();
    assert!(answer(&mut game, &second, vec![handles(&second)[0]]).is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P1", "field").len(), 1, "{state}");
    assert_eq!(ids(&state, "P2", "field").len(), 1, "{state}");
    assert_eq!(
        ids(&state, "P1", "cemetery").len() + ids(&state, "P2", "cemetery").len(),
        2,
        "each player sent away only what it did not keep: {state}"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 24,
        "the inner procedure ran for both players twice"
    );
}

#[test]
fn a_shuffle_naming_the_effect_controller_keeps_that_player() {
    let direct = json!([{"each_player":{"as":"p","stages":[[{"shuffle":{"who":"self"}}]]}}]);
    let wrapped = json!([{"each_player":{"as":"p","stages":[[
        {"if":{"cond":{"cmp":{"left":{"resource":{"who":"self","kind":"pp_max"}},"op":"ge","right":0}},
            "then":[{"shuffle":{"who":"self"}}]}}
    ]]}}]);
    for steps in [direct, wrapped] {
        let (game, question) = play(&steps, (0, 0), (json!([]), json!([])));
        assert!(question.is_none(), "{steps}");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(ids(&state, "P1", "field"), ["s"], "{steps}");
    }
}

#[test]
fn an_unrevealed_hand_choice_is_shown_only_to_its_holder() {
    use sve_engine::game::player::{Input, Viewer};
    let steps = json!([{"each_player":{"as":"p","stages":[[
        {"choose":{"bind":"held","from":{"who":"p","zone":"hand","kind":"follower"},"upto":1,"by":"p"}},
        {"move":{"ref":"held","to":"field","result":"entered"}}
    ]]}}]);
    let (mut game, first) = play(
        &steps,
        (0, 0),
        (
            cards("k", 1, "unit-follower"),
            cards("o", 1, "unit-follower"),
        ),
    );
    let first = first.unwrap();
    assert!(matches!(game.input(Viewer::P2), Input::Waiting(_)));
    let second = answer(&mut game, &first, handles(&first)).unwrap();
    assert_eq!(second.actor, PlayerId { seat: 2 });
    assert!(matches!(game.input(Viewer::P1), Input::Waiting(_)));
    assert!(matches!(game.input(Viewer::P2), Input::Question(_)));
    let seen = game.projection(View::P1).unwrap().to_string();
    assert!(
        !seen.contains("\"o0\""),
        "P1 sees no hidden P2 hand identity"
    );
    let before = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 1 },
        second.token,
        Answer::Targets(handles(&second)),
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    assert!(answer(&mut game, &second, handles(&second)).is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P2", "field"), ["o0"]);
}

#[test]
fn the_effect_controller_draws_each_players_own_hand_count() {
    // P1 holds two cards and P2 one: only P1, the effect controller, draws 2 + 1.
    let steps = json!([{"each_player":{"as":"p","stages":[[
        {"draw":{"who":"self","n":{"count":{"who":"p","zone":"hand"}}}}
    ]]}}]);
    let programs = lowerer()
        .lower(&tree("unit-follower", &json!({"fanfare":{"do":steps}})))
        .unwrap();
    let mut initial = setup(1, json!([]));
    let mut hand = vec![json!({"id":"s","card":"unit-follower"})];
    hand.extend(
        cards("k", 2, "unit-spell")
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    initial["players"]["P1"]["zones"]["hand"] = Value::Array(hand);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P1"]["zones"]["deck"] = cards("P1d", 5, "unit-spell");
    initial["players"]["P2"]["zones"]["hand"] = cards("o", 1, "unit-spell");
    initial["players"]["P2"]["zones"]["deck"] = cards("P2d", 5, "unit-spell");
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "count-draw",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending fanfare");
    };
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, "P1", "hand").len(), 5, "{state}");
    assert_eq!(ids(&state, "P2", "hand"), ["o0"]);
    assert_eq!(ids(&state, "P2", "deck").len(), 5);
}

#[test]
fn effect_controller_actions_and_dice_inside_a_procedure_keep_their_own_actor() {
    for (steps, case) in [
        (json!([{"extra_turn":{"who":"self"}}]), "extra turn"),
        (json!([{"dice":{"bind":"die"}}]), "dice"),
        (json!([{"win":{"who":"self"}}]), "win"),
    ] {
        let procedure = json!([{"each_player":{"as":"p","stages":[steps]}}]);
        let programs = lowerer()
            .lower(&tree("unit-follower", &json!({"fanfare":{"do":procedure}})))
            .unwrap_or_else(|error| panic!("{case}: {error:?}"));
        let body = programs["unit-follower"].card().abilities[0]
            .node
            .body
            .clone()
            .unwrap();
        let Node::Control {
            control: Control::EachPlayer { stages, .. },
        } = &body.steps[0].node
        else {
            panic!("{case}: a player procedure");
        };
        let Node::Instruction { instruction } = &stages[0].body.steps[0].node else {
            panic!("{case}: one stage instruction");
        };
        assert_eq!(
            instruction.actor,
            PlayerRef::Controller {},
            "{case}: the effect controller still acts"
        );
    }
}

#[test]
fn a_nested_procedure_shuffles_the_deck_its_action_names() {
    // Inside q's stage, `who: p` is the outer player, `who: q` the inner one, `self` the
    // effect controller; each shuffle is performed by exactly that player.
    for who in ["p", "q", "self"] {
        let steps = json!([{"each_player":{"as":"p","stages":[[
            {"each_player":{"as":"q","stages":[[{"shuffle":{"who":who}}]]}}
        ]]}}]);
        let (game, question) = play(&steps, (0, 0), (json!([]), json!([])));
        assert!(question.is_none(), "who={who}");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(ids(&state, "P1", "field"), ["s"], "who={who}");
    }
}

#[path = "player_gifts.rs"]
mod gifts;
