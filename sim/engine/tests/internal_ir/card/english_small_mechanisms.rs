use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, TargetRoleAssignment};

/// Printed cards with distinct classes and stats; every number starts without abilities.
const CARDS: [(&str, &str, &str, &str, &str); 9] = [
    ("observer", "アミュレット", "ニュートラル", "-", "-"),
    ("night-a", "フォロワー", "ナイトメア", "2", "3"),
    ("night-b", "フォロワー", "ナイトメア", "2", "3"),
    ("royal", "フォロワー", "ロイヤル", "2", "3"),
    ("big", "フォロワー", "ニュートラル", "4", "5"),
    ("small", "フォロワー", "ニュートラル", "1", "3"),
    ("evolver", "フォロワー", "ニュートラル", "2", "2"),
    ("charm", "アミュレット", "ニュートラル", "-", "-"),
    ("filler", "スペル", "ニュートラル", "-", "-"),
];

fn catalog_for_mechanisms() -> Arc<Catalog> {
    let (_, registry, _) = catalog_documents(&json!("1"));
    let faces = CARDS
        .iter()
        .map(|(number, kind, class, power, hp)| {
            json!({"number":number,"faces":[{"name":number,"card_class":class,"card_type":kind,
                "cost":"1","power":power,"hp":hp,"traits":[],"text":null,"sections":[]}]})
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut effects = json!({"format":1_i32,"kind":"fixture_cards","cards":{}});
    for (number, ..) in CARDS {
        effects["cards"][number] = json!({"status":"complete"});
    }
    Arc::new(
        Catalog::from_fixture_documents(
            &faces,
            &registry,
            &[("mechanisms".into(), effects.to_string())],
        )
        .unwrap(),
    )
}

fn lowered(head: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(head, "observer"))
        .unwrap()
        .remove("observer")
        .unwrap()
}

fn activation(steps: &Value) -> WellFormedCardProgram {
    lowered(&json!({"activate":{"cost":{"pp":0},"do":steps}}))
}

fn board(programs: &[(&str, &WellFormedCardProgram)], zones: [Value; 2]) -> Game {
    let mut initial = setup(1, json!([]));
    for (player, zones) in ["P1", "P2"].into_iter().zip(zones) {
        initial["players"][player]["pp"] = json!({"current":10,"max":10});
        initial["players"][player]["zones"] = zones;
    }
    let mut game = Game::new(
        catalog_for_mechanisms(),
        &initial,
        &Value::Null,
        &Value::Null,
        "english-small-mechanisms",
    )
    .unwrap();
    for (number, ..) in CARDS {
        let program = programs
            .iter()
            .find(|(loaded, _)| *loaded == number)
            .map_or_else(|| card(vec![]), |(_, program)| (*program).clone());
        game.load_internal_card(number, &program).unwrap();
    }
    game
}

fn start(game: &mut Game, program: &WellFormedCardProgram) -> CardPlay {
    game.activate_internal_card(
        "p",
        program.card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap()
}

fn question(play: &CardPlay) -> &DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("a question is expected");
    };
    question
}

fn handles(question: &DecisionView, cards: &[&str]) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target question");
    };
    cards
        .iter()
        .map(|card| {
            domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(*card))
                .unwrap()
                .handle
        })
        .collect()
}

fn pick(game: &mut Game, play: &CardPlay, cards: &[&str]) -> CardPlay {
    let question = question(play);
    let picked = handles(question, cards);
    game.answer_internal_card(question.actor, question.token, Answer::Targets(picked))
        .unwrap()
}

fn finish(game: &mut Game, play: CardPlay) {
    resolved(play);
    game.advance_player_input();
}

/// Runs rule checks and resolves every pending triggered ability without choices.
fn settle(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("a pending ability is expected");
        };
        let play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play);
    }
}

fn referee(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn observer() -> Value {
    json!([{"id":"p","card":"observer"}])
}

#[test]
fn a_hand_choice_by_class_discards_one_card_per_class_and_draws_as_many() {
    let program = activation(&json!([
        {"choose":{"bind":"chosen","from":{"who":"self","zone":"hand"},"distinct_by":"class","any":true}},
        {"discard":{"ref":"chosen","result":"discarded"}},
        {"draw":{"who":"self","n":{"count_of":"discarded"}}}
    ]));
    let deck = (0..3)
        .map(|index| json!({"id":format!("d{index}"),"card":"filler"}))
        .collect::<Vec<_>>();
    let zones = || {
        [
            json!({"field":observer(),"deck":deck,"hand":[
                {"id":"na","card":"night-a"},{"id":"nb","card":"night-b"},{"id":"r","card":"royal"}
            ]}),
            json!({}),
        ]
    };
    let mut game = board(&[("observer", &program)], zones());
    let play = start(&mut game, &program);
    let pending = exact_authority(&game);
    let same_class = handles(question(&play), &["night-a", "night-b"]);
    let asked = question(&play);
    game.answer_internal_card(asked.actor, asked.token, Answer::Targets(same_class))
        .unwrap_err();
    assert_eq!(exact_authority(&game), pending);
    let resolved_play = pick(&mut game, &play, &["night-a", "royal"]);
    finish(&mut game, resolved_play);
    let state = referee(&game);
    assert_eq!(state["P1"]["hand"], json!(["nb", "d0", "d1"]));
    assert_eq!(state["P1"]["cemetery"], json!(["na", "r"]));

    // A large hand is bounded by its class count instead of enumerating every subset.
    let mut large = zones();
    large[0]["hand"] = json!(
        (0..12)
            .map(|index| json!({"id":format!("h{index}"),"card":if index % 2 == 0 {"night-a"} else {"royal"}}))
            .collect::<Vec<_>>()
    );
    let mut game = board(&[("observer", &program)], large);
    let play = start(&mut game, &program);
    let InputDomain::Targets(domain) = &question(&play).domain else {
        panic!("target question");
    };
    assert_eq!(domain.members.len(), 12);
}

#[test]
fn chosen_hand_cards_placed_one_at_a_time_count_for_the_draw() {
    let program = activation(&json!([
        {"choose":{"bind":"first","from":{"who":"self","zone":"hand"},"upto":1}},
        {"move":{"ref":"first","to":"deck","position":"bottom","result":"placed_first"}},
        {"choose":{"bind":"second","from":{"who":"self","zone":"hand"},"upto":1}},
        {"move":{"ref":"second","to":"deck","position":"bottom","result":"placed_second"}},
        {"draw":{"who":"self","n":{"plus":[{"count_of":"placed_first"},{"count_of":"placed_second"}]}}}
    ]));
    for picks in [0_usize, 1, 2] {
        let mut game = board(
            &[("observer", &program)],
            [
                json!({"field":observer(),
                    "deck":[{"id":"d0","card":"filler"},{"id":"d1","card":"filler"},{"id":"d2","card":"filler"}],
                    "hand":[{"id":"na","card":"night-a"},{"id":"r","card":"royal"},{"id":"b","card":"big"}]}),
                json!({}),
            ],
        );
        let mut play = start(&mut game, &program);
        play = pick(&mut game, &play, if picks >= 1 { &["royal"] } else { &[] });
        play = pick(
            &mut game,
            &play,
            if picks >= 2 { &["night-a"] } else { &[] },
        );
        finish(&mut game, play);
        let state = referee(&game);
        let (hand, deck) = match picks {
            0 => (json!(["na", "r", "b"]), json!(["d0", "d1", "d2"])),
            1 => (json!(["na", "b", "d0"]), json!(["d1", "d2", "r"])),
            _ => (json!(["b", "d0", "d1"]), json!(["d2", "r", "na"])),
        };
        assert_eq!(state["P1"]["hand"], hand, "{picks} placed");
        assert_eq!(state["P1"]["deck"], deck, "{picks} placed");
    }
}

#[test]
fn two_distinct_followers_trade_damage_by_each_others_attack() {
    let mut head =
        equipment_played::tree(&json!({"activate":{"cost":{"pp":0},"do":[]}}), "observer");
    head["cards"]["observer"]["abilities"][0]["activate"] = json!({
        "cost":{"pp":0},
        "targets":[{"bind":"pair","from":{"who":"all","zone":"field","kind":"follower"},"n":2,
            "roles":{"first":{"if_nonempty":1},"second":"remaining"}}],
        "do":[
            {"damage":{"to":{"role":{"of":"pair","name":"first"}},"n":{"power_of":{"role":{"of":"pair","name":"second"}}}}},
            {"damage":{"to":{"role":{"of":"pair","name":"second"}},"n":{"power_of":{"role":{"of":"pair","name":"first"}}}}}
        ]
    });
    let roles = lowerer().lower(&head).unwrap().remove("observer").unwrap();
    let mut game = board(
        &[("observer", &roles)],
        [
            json!({"field":[{"id":"p","card":"observer"},{"id":"mine","card":"big"}]}),
            json!({"field":[{"id":"theirs","card":"small"}]}),
        ],
    );
    let play = start(&mut game, &roles);
    let asked = question(&play);
    let InputDomain::Targets(domain) = &asked.domain else {
        panic!("target question");
    };
    assert_eq!(domain.members.len(), 2);
    let [big, small] = handles(asked, &["big", "small"])[..] else {
        panic!("both followers are candidates");
    };
    let answer = |first: u64, second: u64| Answer::TargetsWithRoles {
        targets: vec![first, second],
        roles: vec![
            TargetRoleAssignment {
                role: Nat(0),
                targets: vec![first],
            },
            TargetRoleAssignment {
                role: Nat(1),
                targets: vec![second],
            },
        ],
    };
    let pending = exact_authority(&game);
    game.answer_internal_card(asked.actor, asked.token, answer(big, big))
        .unwrap_err();
    assert_eq!(exact_authority(&game), pending);
    let play = game
        .answer_internal_card(asked.actor, asked.token, answer(small, big))
        .unwrap();
    finish(&mut game, play);
    settle(&mut game);
    let state = referee(&game);
    assert_eq!(state["P2"]["cemetery"], json!(["theirs"]));
    assert_eq!(state["objects"]["mine"]["hp"], 4);

    // Only a role of at most one object lends its attack.
    let mut wide = head.clone();
    wide["cards"]["observer"]["abilities"][0]["activate"]["targets"][0]["n"] = json!(3);
    lowerer().lower(&wide).unwrap_err();
}

#[test]
fn an_entering_follower_with_evolve_is_noticed_once_per_turn() {
    let observer = lowered(&json!({"auto":{"per_turn":1,
        "on":{"enter_field":{"who":"self","sel":{"kind":"follower","has_evolve":true}}},
        "do":[{"draw":{"who":"self","n":1}}]}}));
    let evolver = lowerer()
        .lower(&equipment_played::tree(
            &json!({"evolve":{"cost":{"pp":1}}}),
            "evolver",
        ))
        .unwrap()
        .remove("evolver")
        .unwrap();
    let summoner = lowerer()
        .lower(&equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[
                {"choose":{"bind":"summoned","from":{"who":"self","zone":"hand","kind":"follower"},"upto":1}},
                {"move":{"ref":"summoned","to":"field","result":"entered"}}
            ]}}),
            "charm",
        ))
        .unwrap()
        .remove("charm")
        .unwrap();
    let mut game = board(
        &[
            ("observer", &observer),
            ("evolver", &evolver),
            ("charm", &summoner),
        ],
        [
            json!({"field":[{"id":"p","card":"observer"},{"id":"c","card":"charm"}],
                "deck":[{"id":"d0","card":"filler"},{"id":"d1","card":"filler"}],
                "hand":[{"id":"plain","card":"small"},{"id":"e1","card":"evolver"},{"id":"e2","card":"evolver"}]}),
            json!({}),
        ],
    );
    let summon = |game: &mut Game, card: &str| {
        let play = game
            .activate_internal_card(
                "c",
                summoner.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap();
        let play = pick(game, &play, &[card]);
        finish(game, play);
        settle(game);
    };
    summon(&mut game, "small");
    assert_eq!(referee(&game)["P1"]["hand"], json!(["e1", "e2"]));
    summon(&mut game, "evolver");
    assert_eq!(referee(&game)["P1"]["hand"], json!(["e2", "d0"]));
    summon(&mut game, "evolver");
    assert_eq!(referee(&game)["P1"]["hand"], json!(["d0"]));
}

#[test]
fn a_conditional_reveal_is_read_inside_its_branch() {
    let steps = |le: u8| {
        json!([{"if":{"cond":{"leader_life":{"who":"self","le":le}},"then":[
            {"reveal":{"top":{"who":"self","n":3},"bind":"shown"}},
            {"choose":{"bind":"summoned","from":{"in":"shown","kind":"follower"},"may":true,"upto":3}},
            {"move":{"ref":"summoned","to":"field","result":"entered"}},
            {"move":{"ref":{"in":"shown","except":{"in":{"objects_of":"entered","view":"before"}}},
                "to":"deck","position":"bottom","order":"shuffle"}}
        ]}}])
    };
    let deck = json!([
        {"id":"f1","card":"small"},{"id":"s1","card":"filler"},{"id":"f2","card":"big"},
        {"id":"rest","card":"filler"}
    ]);
    let program = activation(&steps(20));
    let mut game = board(
        &[("observer", &program)],
        [json!({"field":observer(),"deck":deck}), json!({})],
    );
    let play = start(&mut game, &program);
    let play = pick(&mut game, &play, &["small"]);
    finish(&mut game, play);
    let state = referee(&game);
    assert_eq!(state["P1"]["field"], json!(["p", "f1"]));
    let rest = state["P1"]["deck"].as_array().unwrap();
    assert_eq!(rest[0], json!("rest"));
    assert_eq!(rest.len(), 3);

    // A false condition reveals nothing.
    let program = activation(&steps(1));
    let mut game = board(
        &[("observer", &program)],
        [json!({"field":observer(),"deck":deck}), json!({})],
    );
    let play = start(&mut game, &program);
    finish(&mut game, play);
    assert_eq!(
        referee(&game)["P1"]["deck"],
        json!(["f1", "s1", "f2", "rest"])
    );
}

#[test]
fn each_player_may_return_an_amulet_from_their_own_cemetery() {
    let program = activation(&json!([{"each_player":{"as":"player","stages":[[
        {"choose":{"bind":"recovered","by":"player","from":{"who":"player","zone":"cemetery","kind":"amulet"},"upto":1}},
        {"move":{"ref":"recovered","to":"hand"}}
    ]]}}]));
    let mut game = board(
        &[("observer", &program)],
        [
            json!({"field":observer(),"cemetery":[{"id":"mine","card":"charm"},{"id":"dead","card":"small"}]}),
            json!({"cemetery":[{"id":"theirs","card":"charm"}]}),
        ],
    );
    let play = start(&mut game, &program);
    assert_eq!(question(&play).actor, PlayerId { seat: 1 });
    let play = pick(&mut game, &play, &["charm"]);
    let asked = question(&play);
    assert_eq!(asked.actor, PlayerId { seat: 2 });
    let play = pick(&mut game, &play, &[]);
    finish(&mut game, play);
    let state = referee(&game);
    assert_eq!(state["P1"]["hand"], json!(["mine"]));
    assert_eq!(state["P2"]["cemetery"], json!(["theirs"]));

    // Without the holder choosing, another player's cemetery stays closed.
    let mut controller = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[{"each_player":{"as":"player","stages":[[
            {"choose":{"bind":"recovered","from":{"who":"player","zone":"cemetery","kind":"amulet"},"upto":1}},
            {"move":{"ref":"recovered","to":"hand"}}
        ]]}}]}}),
        "observer",
    );
    lowerer().lower(&controller).unwrap_err();
    controller["cards"]["observer"]["abilities"][0]["activate"]["do"][0]["each_player"]["stages"]
        [0][0]["choose"]["by"] = json!("player");
    lowerer().lower(&controller).unwrap();
}
