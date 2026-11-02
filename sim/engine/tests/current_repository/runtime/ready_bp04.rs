#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
type SupportCard = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const CONTROLS: [SupportCard; 13] = [
    (
        "medusa-print",
        "毒牙の姫・メドゥーサ",
        "ナイトメア",
        "フォロワー",
        "3",
        &[],
    ),
    (
        "beater",
        "デモンビーター",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "neutral-one",
        "neutral one",
        "ニュートラル",
        "フォロワー",
        "1",
        &["ゴブリン"],
    ),
    (
        "neutral-two",
        "neutral two",
        "ニュートラル",
        "フォロワー",
        "2",
        &[],
    ),
    ("royal-one", "royal one", "ロイヤル", "フォロワー", "1", &[]),
    ("witch-two", "witch two", "ウィッチ", "フォロワー", "2", &[]),
    ("witch-spell", "witch spell", "ウィッチ", "スペル", "1", &[]),
    (
        "nightmare-one",
        "nightmare one",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "nightmare-three",
        "nightmare three",
        "ナイトメア",
        "フォロワー",
        "3",
        &[],
    ),
    (
        "amulet-one",
        "amulet one",
        "ニュートラル",
        "アミュレット",
        "1",
        &[],
    ),
    ("steno", "ステンノ", "ナイトメア", "フォロワー", "1", &[]),
    (
        "euryale",
        "エウリュアレー",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    ("old-spell", "old spell", "ニュートラル", "スペル", "1", &[]),
];
// Synthetic supports isolate filters without admitting unrelated card effects.
fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower=kind=="フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if follower {"2"} else {"-"},"hp":if follower {"3"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp04",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(
            number,
            if number == "neutral-two" {
                destroyer_program()
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|i| json!({"id":format!("deck{i}"),"card":"neutral-one"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8,"acted":true}}]);
    initial
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => {
                assert_eq!(
                    d.cards.len(),
                    1,
                    "only the intended card matches the printed search"
                );
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
            }
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected printed question: {:?}", q.domain),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the printed action has its resources"
    );
    checks(game);
}
fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step);
}
fn pending(game: &mut Game) -> CardPlay {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed trigger is pending"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}
fn trigger(game: &mut Game) {
    let step = pending(game);
    finish(game, step);
}
fn activate_card(game: &mut Game, number: &str, line: u32) {
    let id = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, PLAYER).unwrap();
    finish(game, step);
}
fn assert_disclosed_cards(game: &Game, shown: &[&str], private: &[&str]) {
    let view = game.projection(View::P2).unwrap();
    let known = view["known_cards"].as_array().unwrap();
    for id in shown {
        assert!(
            known.iter().any(|card| card["id"] == *id),
            "the chosen card is disclosed to the opponent"
        );
    }
    for id in private {
        assert!(
            !known.iter().any(|card| card["id"] == *id),
            "the untaken deck cards remain private"
        );
    }
}
fn witch_look(number: &str) {
    for chain in [9_u8, 10] {
        let mut initial = position(number, "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..chain)
                .map(|i| json!({"id":format!("old{i}"),"card":"old-spell"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong-class","card":"royal-one"},{"id":"wrong-kind","card":"witch-spell"},{"id":"chosen","card":"witch-two"},{"id":"rest1","card":"neutral-one"},{"id":"rest2","card":"neutral-one"},{"id":"unseen","card":"neutral-one"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            json!(["chosen"]),
            "only the chosen Witch follower enters hand"
        );
        assert_disclosed_cards(
            &game,
            &["chosen"],
            &["wrong-class", "wrong-kind", "rest1", "rest2", "unseen"],
        );
        assert_eq!(
            state["P1"]["deck"],
            json!(["unseen", "rest2", "rest1", "wrong-kind", "wrong-class"]),
            "all untaken cards follow the chosen bottom order"
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            if chain == 10 { 9_u8 } else { 8 },
            "Spellchain ten recovers one PP after ordering the remaining cards"
        );
    }
}
#[test]
fn call_chain_reveals_only_a_witch_follower_and_recovers_pp_at_spellchain_ten() {
    witch_look("BP04-044");
}
#[test]
fn noble_guidance_reveals_only_a_witch_follower_and_recovers_pp_at_spellchain_ten() {
    witch_look("BP04-045");
}
fn hector(number: &str) {
    for graves in [9_u8, 10, 20] {
        let mut initial = position(number, "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"neutral-one"}]);
        let mut cemetery = vec![
            json!({"id":"wrong-cost","card":"nightmare-three"}),
            json!({"id":"revive1","card":"nightmare-one"}),
            json!({"id":"revive2","card":"nightmare-one"}),
        ];
        cemetery.extend((3..graves).map(|i| json!({"id":format!("old{i}"),"card":"neutral-one"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
        let mut game = game(&initial);
        play(&mut game, "source");
        let before = game.projection(View::Referee).unwrap();
        let mut step = pending(&mut game);
        if graves >= 10 {
            let CardPlay::Awaiting(question) = step else {
                panic!("the conditional cemetery choice")
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("the cemetery candidates")
            };
            assert_eq!(
                domain.members.len(),
                2,
                "only cost-two-or-less Nightmare followers are eligible"
            );
            assert_eq!(
                domain.cardinality.maximum,
                Some(Nat(2)),
                "at most two followers return"
            );
            step = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
                )
                .unwrap();
        }
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        let bonus = if graves == 20 {
            3_u8
        } else {
            u8::from(graves == 10)
        };
        assert_eq!(
            state["objects"]["source"]["power"], before["objects"]["source"]["power"],
            "the buff excludes its source"
        );
        assert_eq!(
            state["objects"]["ally"]["power"],
            2_u8.checked_add(bonus).unwrap(),
            "the buff includes other Neutral followers"
        );
        assert!(
            !state["objects"]["ally"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "the Rush aura excludes other classes"
        );
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "the Nightmare source receives its own aura"
        );
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(graves)
                .checked_sub(if graves >= 10 { 2 } else { 0 })
                .unwrap(),
            "only the revived followers leave the starting cemetery"
        );
        if graves >= 10 {
            for id in ["revive1", "revive2"] {
                assert_eq!(
                    state["objects"][id]["power"],
                    2_u8.checked_add(bonus).unwrap(),
                    "Q614 keeps the starting Necrocharge tier after revival"
                );
                assert!(
                    state["objects"][id]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("突進")),
                    "the revived Nightmare followers receive Rush"
                );
            }
        } else {
            assert_eq!(
                state["P1"]["field"],
                json!(["ally", "source"]),
                "Necrocharge nine leaves only the source and prior ally"
            );
        }
    }
}
#[test]
fn hector_revives_two_then_uses_the_starting_necrocharge_tier_for_its_other_followers() {
    hector("BP04-082");
}
#[test]
fn black_knights_commander_uses_the_same_necrocharge_and_aura_behaviour() {
    hector("BP04-083");
}
#[test]
fn emperor_uses_the_same_necrocharge_and_aura_behaviour() {
    hector("BP04-SP01");
}
// Simultaneous field entries have no printed ordering requirement here.
fn assert_field_members(state: &Value, expected: &[&str]) {
    let actual = state["P1"]["field"].as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "the field contains exactly the cards allowed by the printed search"
    );
    for id in expected {
        assert!(
            actual.contains(&json!(id)),
            "each chosen name enters the field"
        );
    }
}
#[test]
fn gorgon_trial_searches_each_named_group_and_can_decline_one_name() {
    for decline in [false, true] {
        let mut initial = position("BP04-086", "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"medusa","card":"medusa-print"}, {"id":"steno","card":"steno"},
            {"id":"euryale","card":"euryale"}, {"id":"wrong","card":"neutral-one"}
        ]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game.play_internal_card("source", PLAYER).unwrap()
        else {
            panic!("grouped search")
        };
        let InputDomain::Search(domain) = &question.domain else {
            panic!("search domain")
        };
        let groups = domain.groups.as_ref().unwrap();
        assert_eq!(groups.len(), 3);
        assert!(
            groups.iter().all(|group| group.handles.len() == 1),
            "each name has its own candidate"
        );
        let answer = groups
            .iter()
            .enumerate()
            .map(|(i, group)| {
                if decline && i == 2 {
                    vec![]
                } else {
                    group.handles.clone()
                }
            })
            .collect();
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Grouped(answer)),
            )
            .unwrap();
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        assert_field_members(
            &state,
            if decline {
                &["medusa", "steno"]
            } else {
                &["medusa", "steno", "euryale"]
            },
        );
        assert_eq!(state["P1"]["deck_count"], if decline { 2_u8 } else { 1 });
    }
}

#[test]
fn gorgon_trial_lets_the_searcher_choose_which_two_names_fit_the_field() {
    let mut initial = position("BP04-086", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"old1","card":"neutral-one"}, {"id":"old2","card":"neutral-one"},
        {"id":"old3","card":"neutral-one"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"medusa","card":"medusa-print"}, {"id":"steno","card":"steno"},
        {"id":"euryale","card":"euryale"}, {"id":"wrong","card":"neutral-one"}
    ]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game.play_internal_card("source", PLAYER).unwrap() else {
        panic!("grouped search")
    };
    let InputDomain::Search(search_domain) = &question.domain else {
        panic!("search domain")
    };
    let groups = search_domain.groups.as_ref().unwrap();
    let allocation = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Grouped(
                groups.iter().map(|g| g.handles.clone()).collect(),
            )),
        )
        .unwrap();
    let CardPlay::Awaiting(capacity) = allocation else {
        panic!("field capacity choice")
    };
    let InputDomain::SearchPartition(partition_domain) = &capacity.domain else {
        panic!("search allocation")
    };
    assert_eq!(partition_domain.cardinality.maximum, Some(Nat(2)));
    let selected = partition_domain
        .cards
        .iter()
        .filter(|c| c.card != "medusa-print")
        .map(|c| c.handle)
        .collect();
    let continuation = game
        .answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(selected),
        )
        .unwrap();
    finish(&mut game, continuation);
    let state = game.projection(View::Referee).unwrap();
    assert_field_members(&state, &["old1", "old2", "old3", "steno", "euryale"]);
    assert_eq!(
        state["P1"]["deck_count"], 2_u8,
        "the unfitted name remains in the deck"
    );
}

fn destroyer_program() -> &'static WellFormedCardProgram {
    static PROGRAM: OnceLock<WellFormedCardProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let base = &tree["cards"]["synthetic-evolution"];
        let mut ability = base["abilities"][0].clone();
        ability.as_object_mut().unwrap().remove("evolve");
        ability["activate"] = json!({"cost":{"pp":0_u8},"do":[
            {"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}
        ]});
        tree["cards"] = json!({"neutral-two":{"meta":base["meta"],"abilities":[ability]}});
        lowerer()
            .lower(&tree)
            .unwrap()
            .remove("neutral-two")
            .unwrap()
    })
}

fn destroy_own_followers(game: &mut Game) {
    let program = destroyer_program();
    let step = game
        .activate_internal_card("killer", program.card().abilities[0].node.id, PLAYER)
        .unwrap();
    finish(game, step);
}

#[test]
fn demon_beater_pays_two_to_find_its_name_then_lastword_heals_its_owner() {
    let mut initial = position("BP04-092", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"found","card":"beater"}, {"id":"wrong","card":"nightmare-one"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"killer","card":"neutral-two"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["killer", "source", "found"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護"))
    );
    destroy_own_followers(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        11_u8
    );
}

#[test]
fn stairway_counts_each_fallen_follower_and_requires_six_before_looking_five() {
    let mut initial = position("BP04-122", "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"counters":{"魂カウンター":4_u8}});
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"killer","card":"neutral-two"}),
            json!({"id":"fallen","card":"neutral-one"}),
        ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"spell","card":"witch-spell"}, {"id":"first","card":"neutral-one"},
        {"id":"second","card":"nightmare-one"}, {"id":"amulet","card":"amulet-one"},
        {"id":"rest","card":"old-spell"}, {"id":"unseen","card":"neutral-one"}
    ]);
    let mut game = game(&initial);
    let ability = selection().current["BP04-122"].program.card().abilities[1]
        .node
        .id;
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", ability, PLAYER)
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "insufficient counters preserve the source and resources"
    );
    destroy_own_followers(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source"]),
        "only the two followers fall; the counter holder survives"
    );
    for _ in 0..2_u8 {
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("counter triggers")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(&mut game, step);
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["魂カウンター"],
        6_u8
    );
    activate_card(&mut game, "BP04-122", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
    assert_disclosed_cards(
        &game,
        &["first", "second"],
        &["spell", "amulet", "rest", "unseen"],
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "rest", "amulet", "spell"])
    );
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
}

#[test]
fn goblin_princess_finds_only_a_neutral_one_cost_follower() {
    let mut initial = position("BP04-124", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong-class","card":"royal-one"}, {"id":"wrong-cost","card":"neutral-two"},
        {"id":"wrong-kind","card":"amulet-one"}, {"id":"found","card":"neutral-one"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source", "found"])
    );
}

#[test]
fn goblin_princess_evolution_creates_the_printed_king_in_ex() {
    let mut initial = position("BP04-124", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-125"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1);
    let king = &state["objects"][ex[0].as_str().unwrap()];
    assert_eq!(king["name"], "ゴブリンキング");
    assert_eq!(king["power"], 6_u8);
    assert_eq!(king["hp"], 6_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn goblin_king_buffs_other_goblins_and_enters_with_ward() {
    let mut initial = position("BP04-T02", "ex");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"goblin","card":"neutral-one"}, {"id":"other","card":"royal-one"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["goblin"]["power"], 3_u8);
    assert_eq!(state["objects"]["goblin"]["hp"], 4_u8);
    assert_eq!(state["objects"]["other"]["power"], 2_u8);
    assert_eq!(state["objects"]["source"]["power"], 6_u8);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護"))
    );
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}
