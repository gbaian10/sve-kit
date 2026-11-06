#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the named-card entry scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

const CASES: &[(&str, &str, &str)] = &[
    ("BP08-040", "マジカルビースト・モーラ", "フォロワー"),
    ("BP21-013", "ソードラビット", "フォロワー"),
    ("BP19-027", "静かなるメイド・ノーニャ", "フォロワー"),
    ("BP08-021", "不滅の聖剣・デュランダル", "アミュレット"),
];
fn face(name: &str, kind: &str, maid: bool) -> Value {
    let follower = kind == "フォロワー";
    json!({"name":name,"card_type":kind,"card_class":"ニュートラル","cost":"1",
        "power":if follower {"2"} else {"-"},"hp":if follower {"10"} else {"-"},
        "traits":if maid {vec!["メイド"]} else {vec![]},"text":Value::Null,"sections":[]})
}
fn fixture() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static FIXTURE: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut controls = vec![
            ("named-plain".to_owned(), face("plain", "スペル", false)),
            ("named-enemy".to_owned(), face("enemy", "フォロワー", false)),
            ("named-bouncer".to_owned(), face("bouncer", "アミュレット", false)),
            ("named-plain-follower".to_owned(), face("no Storm", "フォロワー", false)),
            ("BP19-027-trait".to_owned(), face("静かなるメイド・ノーニャ", "フォロワー", false)),
        ];
        for (number, name, kind) in CASES {
            let maid = *number == "BP19-027";
            let wrong_kind = if *kind == "フォロワー" { "アミュレット" } else { "フォロワー" };
            controls.extend([
                (format!("{number}-match"), face(name, kind, maid)),
                (format!("{number}-name"), face("wrong name", kind, maid)),
                (format!("{number}-kind"), face(name, wrong_kind, maid)),
            ]);
        }
        for (number, card_face) in controls {
            snapshot.push_str(&json!({"number":number,"faces":[card_face]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let mut tree = expanded::tree().clone();
        let mut producer = tree["cards"]["synthetic-evolution"].clone();
        producer["abilities"][0].as_object_mut().unwrap().remove("evolve");
        producer["abilities"][0]["activate"] = json!({"cost":{"pp":0_u8},"do":[{"move":{"ref":{"who":"self","zone":"field","kind":"follower"},"to":"hand"}}]});
        tree["cards"] = json!({"named-bouncer":producer});
        (repository::selection_with_rules_from(&snapshot,&keywords,&input),lowerer().lower(&tree).unwrap())
    })
}
fn build(initial: &Value) -> Game {
    let (selected, programs) = fixture();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "named-hand-entry",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for name in ["named-plain", "named-enemy", "named-plain-follower"] {
        game.load_internal_card(name, &empty).unwrap();
    }
    for (number, ..) in CASES {
        for suffix in ["match", "name", "kind"] {
            game.load_internal_card(&format!("{number}-{suffix}"), &empty)
                .unwrap();
        }
    }
    game.load_internal_card("BP19-027-trait", &empty).unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}
fn finish(game: &mut Game, mut play: CardPlay, mode: u64, trace: &mut Vec<Vec<String>>) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                trace.push(
                    domain
                        .members
                        .iter()
                        .filter_map(|member| member.card.clone())
                        .collect(),
                );
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => {
                trace.push(domain.cards.iter().map(|card| card.card.clone()).collect());
                let mut preferred = domain.cards.iter().collect::<Vec<_>>();
                preferred.sort_by_key(|card| !card.card.ends_with("-match"));
                Answer::Search(SearchAnswer::Ungrouped(
                    preferred
                        .iter()
                        .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                        .map(|card| card.handle)
                        .collect(),
                ))
            }
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            _ => panic!("unexpected named entry choice: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed instruction resolves"
    );
}
fn settle(game: &mut Game, mode: u64, trace: &mut Vec<Vec<String>>) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending named entry trigger")
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, mode, trace);
    }
}
fn position(number: &str, alias: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":alias},{"id":"matching","card":format!("{number}-match")},
        {"id":"wrong-name","card":format!("{number}-name")},{"id":"wrong-kind","card":format!("{number}-kind")}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"found","card":format!("{number}-match")},{"id":"deck-name","card":format!("{number}-name")},{"id":"deck-kind","card":format!("{number}-kind")},{"id":"plain","card":"named-plain"}
    ]);
    if number == "BP19-027" {
        initial["players"]["P1"]["zones"]["deck"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"deck-trait","card":"BP19-027-trait"}));
    }
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"named-enemy"}]);
    if number == "BP21-013" {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":alias},{"id":"producer","card":"named-bouncer"}]);
    }
    initial
}
fn candidates(trace: &[Vec<String>], mut expected: Vec<String>) {
    let mut actual = trace.last().unwrap().clone();
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "the printed name and kind expose exactly the legal candidates"
    );
}
fn activate(game: &mut Game, number: &str, trace: &mut Vec<Vec<String>>) {
    let (source, ability) = if number == "BP21-013" {
        (
            "producer",
            fixture().1["named-bouncer"].card().abilities[0].node.id,
        )
    } else {
        (
            "source",
            fixture().0.current[number]
                .program
                .card()
                .abilities
                .iter()
                .find(|ability| ability.node.kind == AbilityKind::Activated)
                .unwrap()
                .node
                .id,
        )
    };
    let play = game
        .activate_internal_card(source, ability, PlayerId { seat: 1 })
        .unwrap();
    finish(game, play, 1, trace);
    settle(game, 1, trace);
}
fn verify(game: &mut Game, number: &str, alias: &str, mode: u64, trace: &mut Vec<Vec<String>>) {
    let matching = format!("{number}-match");
    if number == "BP19-027" {
        candidates(trace, vec![matching.clone(), format!("{number}-name")]);
        activate(game, number, trace);
        candidates(trace, vec![matching.clone(), matching]);
    } else if number == "BP21-013" {
        candidates(trace, vec![matching, alias.to_owned()]);
    } else {
        candidates(trace, vec![matching]);
    }
    let entered = if mode == 1 && matches!(number, "BP08-021" | "BP08-040") {
        "found"
    } else {
        "matching"
    };
    if number == "BP08-040" {
        activate(game, number, trace);
    }
    let state = game.projection(View::Referee).unwrap();
    let destination = if number == "BP08-021" && mode == 1 {
        "hand"
    } else {
        "field"
    };
    assert!(
        state["P1"][destination]
            .as_array()
            .unwrap()
            .contains(&json!(entered)),
        "the selected named card reaches the printed destination"
    );
    for rejected in ["wrong-name", "wrong-kind"] {
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!(rejected)),
            "the name or kind counterexample stays in hand"
        );
    }
    if number == "BP08-040" {
        assert_eq!(
            state["objects"]["enemy"]["hp"], 7_u8,
            "Monika damages the chosen enemy for three"
        );
        for material in ["source", entered] {
            assert_eq!(
                state["objects"][material]["acted"], true,
                "both printed Act costs are paid"
            );
        }
    }
    if number == "BP19-027" {
        assert_eq!(
            state["P1"]["pp"]["current"], 7_u8,
            "playing Prim and activating her pays two plus one PP"
        );
        assert!(
            state["objects"][entered]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "Prim grants Ward to the entered follower"
        );
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("found")),
            "the Maid search adds its card before activation"
        );
    }
}
fn run(number: &str) {
    let selected = &fixture().0;
    let identity = &selected.current[number].card_id;
    for (alias, _) in selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
    {
        let modes = if matches!(number, "BP08-021" | "BP08-040") {
            vec![1_u64, 2]
        } else {
            vec![1_u64]
        };
        for mode in modes {
            let mut game = build(&position(number, alias));
            let mut trace = vec![];
            if number == "BP21-013" {
                activate(&mut game, number, &mut trace);
            } else {
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play, mode, &mut trace);
                settle(&mut game, mode, &mut trace);
            }
            verify(&mut game, number, alias, mode, &mut trace);
        }
    }
}
#[test]
fn bp08_040_resolves_both_named_entry_modes_and_the_two_act_costs() {
    run("BP08-040");
}
#[test]
fn bp21_013_bounce_can_select_the_returned_named_source() {
    run("BP21-013");
}
#[test]
fn bp19_027_searches_a_maid_then_grants_ward_to_the_named_entry() {
    run("BP19-027");
}
#[test]
fn bp08_021_resolves_both_named_amulet_modes() {
    run("BP08-021");
}

fn any_position(source: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":source}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..8)
            .map(|i| json!({"id":format!("d{i}"),"card":"named-plain"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"named-enemy","state":{"acted":true}}]);
    initial
}
fn answer_any_entry(game: &mut Game, play: CardPlay, maximum: u64, mut expected: Vec<String>) {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected the private any-card entry question")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected hand targets")
    };
    assert_eq!(
        domain.cardinality.minimum.0, 0_u64,
        "any permits choosing zero cards"
    );
    assert_eq!(
        domain.cardinality.maximum.unwrap().0,
        maximum,
        "the private any bound equals the free field slots"
    );
    let mut actual = domain
        .members
        .iter()
        .filter_map(|member| member.card.clone())
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "any offers exactly the eligible hand cards"
    );
    let answer = Answer::Targets(
        domain
            .members
            .iter()
            .take(usize::try_from(maximum).unwrap())
            .map(|member| member.handle)
            .collect(),
    );
    let next = game
        .answer_internal_card(question.actor, question.token, answer)
        .unwrap();
    let mut trace = vec![];
    finish(game, next, 1, &mut trace);
    settle(game, 1, &mut trace);
}
fn castle(number: &str) {
    let selected = &fixture().0;
    let identity = &selected.current[number].card_id;
    for (alias, _) in selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
    {
        let mut initial = any_position(alias);
        let mut hand = (0_u8..6)
            .map(|i| json!({"id":format!("storm{i}"),"card":"BP01-048"}))
            .collect::<Vec<_>>();
        hand.extend([
            json!({"id":"no-storm","card":"named-plain-follower"}),
            json!({"id":"spell","card":"named-plain"}),
            json!({"id":"conditional-storm","card":"BP03-028"}),
        ]);
        initial["players"]["P1"]["zones"]["hand"] = json!(hand);
        let mut game = build(&initial);
        let ability = selected.current[number]
            .program
            .card()
            .abilities
            .iter()
            .rev()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let play = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap();
        answer_any_entry(&mut game, play, 5, vec!["BP01-048".to_owned(); 6]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            5_usize,
            "only five Storm followers enter after the source cost leaves"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 0_u8,
            "the activation pays ten PP"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the source pays its own cemetery cost"
        );
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("storm5")),
            "the sixth eligible follower stays in hand"
        );
        for i in 0_u8..5 {
            let id = format!("storm{i}");
            assert_eq!(
                state["objects"][&id]["power"], 5_u8,
                "every entered Storm follower gains two power"
            );
            assert_eq!(
                state["objects"][&id]["hp"], 5_u8,
                "every entered Storm follower gains two health"
            );
        }
    }
}
#[test]
fn bp03_025_any_storm_entry_uses_free_slots_and_buffs_every_entry() {
    castle_search_and_buff("BP03-025");
    castle("BP03-025");
}
#[test]
fn bp03_026_any_storm_entry_uses_free_slots_and_buffs_every_entry() {
    castle_search_and_buff("BP03-026");
    castle("BP03-026");
}

fn castle_search_and_buff(number: &str) {
    let selected = &fixture().0;
    let identity = &selected.current[number].card_id;
    for (alias, _) in selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
    {
        let mut initial = any_position("BP01-048");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"BP01-048"},{"id":"conditional","card":"BP03-028"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":alias}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found-storm","card":"BP01-048"},{"id":"not-printed-storm","card":"BP03-028"},{"id":"not-a-follower","card":"named-plain"}]);
        let mut game = build(&initial);
        let mut trace = vec![];
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, 1, &mut trace);
        settle(&mut game, 1, &mut trace);
        candidates(&trace, vec!["BP01-048".to_owned()]);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("found-storm")),
            "the Fanfare searches only a printed Storm follower"
        );
        activate(&mut game, number, &mut trace);
        candidates(&trace, vec!["BP01-048".to_owned()]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["ally"]["power"], 4_u8,
            "the first activation gives the currently Storm ally one power"
        );
        assert_eq!(
            state["objects"]["ally"]["hp"], 3_u8,
            "the first activation does not change health"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the first activation pays its source cemetery cost"
        );
    }
}
