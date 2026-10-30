#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed search scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{SearchAnswer, SearchDomain};

struct Case {
    card: &'static str,
    traits: &'static [&'static str],
    limit: u8,
    spell: bool,
    evolution_name: Option<&'static str>,
}
const CASES: [Case; 15] = [
    Case {
        card: "BP05-005",
        traits: &["狩人"],
        limit: 2,
        spell: false,
        evolution_name: Some("不殺の使徒"),
    },
    Case {
        card: "BP10-040",
        traits: &["ゴーレム"],
        limit: 6,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP10-103",
        traits: &["アルカナ"],
        limit: 4,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP12-110",
        traits: &["コック"],
        limit: 3,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP13-062",
        traits: &["武闘竜人"],
        limit: 3,
        spell: false,
        evolution_name: Some("炎柱の竜人"),
    },
    Case {
        card: "BP18-036",
        traits: &[],
        limit: 1,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP19-065",
        traits: &["八獄"],
        limit: 1,
        spell: false,
        evolution_name: Some("倒壊の襲撃者"),
    },
    Case {
        card: "BP19-095",
        traits: &["信仰"],
        limit: 1,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP20-007",
        traits: &["絶傑", "狩人"],
        limit: 3,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP20-031",
        traits: &["兵士"],
        limit: 2,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "BP21-020",
        traits: &["学院"],
        limit: 2,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "CP02-048",
        traits: &[],
        limit: 3,
        spell: true,
        evolution_name: Some("城ヶ崎莉嘉"),
    },
    Case {
        card: "CP04-062",
        traits: &["プリコネ"],
        limit: 2,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "CSD02b-004",
        traits: &["クール"],
        limit: 2,
        spell: false,
        evolution_name: None,
    },
    Case {
        card: "DSD01b-007",
        traits: &["武闘竜人"],
        limit: 4,
        spell: false,
        evolution_name: None,
    },
];
fn empty() -> WellFormedCardProgram {
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap()
}
fn companion(
    snapshot: &mut String,
    input: &mut Value,
    number: &str,
    name: &str,
    spell: bool,
    cost: u8,
    traits: &[&str],
) {
    snapshot.push_str(&json!({"number":number,"faces":[{
        "name":name,"card_class":"ニュートラル","card_type":if spell {"スペル"} else {"フォロワー"},
        "traits":traits,"cost":cost.to_string(),"power":if spell {"-"} else {"1"},
        "hp":if spell {"-"} else {"3"},"text":null,"sections":[]
    }]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([
        format!("{number}.yaml"),
        json!({
            "format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}
        })
        .to_string()
    ]));
}
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = BTreeMap::new();
        for case in &CASES {
            for (suffix, cost, spell, traits) in [
                ("good", case.limit, case.spell, case.traits),
                ("costly", case.limit.checked_add(1).unwrap(), case.spell, case.traits),
                ("kind", case.limit, !case.spell, case.traits),
            ] {
                let number = format!("{}-{suffix}", case.card);
                companion(
                    &mut snapshot,
                    &mut input,
                    &number,
                    &number,
                    spell,
                    cost,
                    traits,
                );
                programs.insert(number, empty());
            }
            if !case.traits.is_empty() {
                let number = format!("{}-trait", case.card);
                let traits = if case.traits.len() > 1 {
                    &case.traits[..1]
                } else {
                    &[]
                };
                companion(
                    &mut snapshot,
                    &mut input,
                    &number,
                    &number,
                    case.spell,
                    case.limit,
                    traits,
                );
                programs.insert(number, empty());
            }
            if let Some(name) = case.evolution_name {
                let number = format!("{}-base", case.card);
                companion(&mut snapshot, &mut input, &number, name, false, 0, &[]);
                let mut tree = expanded::tree().clone();
                tree["cards"] = json!({number.clone():tree["cards"]["synthetic-evolution"]});
                programs.insert(
                    number.clone(),
                    lowerer().lower(&tree).unwrap().remove(&number).unwrap(),
                );
            }
        }
        companion(
            &mut snapshot,
            &mut input,
            "producer",
            "external destruction",
            false,
            0,
            &[],
        );
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        (selected, programs)
    })
}
fn fixture(initial: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cost-batch",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    let mut objects = subjects();
    if let Selector::Zone { players, .. } = &mut objects {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    game.load_internal_card("producer", &producer(Action::Destroy { objects }))
        .unwrap();
    game
}
fn settle(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            break;
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn trigger(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap()
        .handle;
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap();
    settle(game, play)
}
fn enter(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(settle(game, play), CardPlay::Entered(_)),
        "the real source enters"
    );
    trigger(game, number, line)
}
fn found(game: &mut Game, play: CardPlay, expected: &[&str]) {
    let CardPlay::Awaiting(question) = play else {
        panic!("search choice")
    };
    let InputDomain::Search(SearchDomain {
        cards, cardinality, ..
    }) = &question.domain
    else {
        panic!("search domain")
    };
    let mut actual = cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    actual.sort_unstable();
    assert_eq!(
        actual, expected,
        "the private domain contains exactly the qualifying printings"
    );
    assert_eq!(
        cardinality.minimum,
        Nat(0),
        "conditional private search retains its find-failure exception"
    );
    let answer = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![cards[0].handle])),
        )
        .unwrap();
    resolved(settle(game, answer));
}
fn position(number: &str, deck: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = deck;
    initial
}

fn initial(number: &str) -> Value {
    let case = CASES.iter().find(|c| c.card == number).unwrap();
    let mut deck = vec![
        json!({"id":"good","card":format!("{number}-good")}),
        json!({"id":"costly","card":format!("{number}-costly")}),
        json!({"id":"kind","card":format!("{number}-kind")}),
    ];
    if !case.traits.is_empty() {
        deck.push(json!({"id":"trait","card":format!("{number}-trait")}));
    }
    let mut state = position(number, json!(deck));
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    if case.evolution_name.is_some() {
        state["players"]["P1"]["zones"]["hand"] = json!([]);
        state["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":format!("{number}-base")}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    }
    state
}
fn evolve(game: &mut Game, number: &str, line: u64) -> CardPlay {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    trigger(game, number, line)
}
fn search(game: &mut Game, play: CardPlay, number: &str, zone: &str) {
    found(game, play, &[&format!("{number}-good")]);
    let projection = game.projection(View::Referee).unwrap();
    assert!(
        projection["P1"][zone]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "good"),
        "the selected card reaches the printed destination"
    );
    assert!(
        projection["objects"]
            .as_object()
            .unwrap()
            .get("costly")
            .is_none_or(|o| o["zone"] != "field"),
        "a card above the printed cost bound cannot enter"
    );
}
fn mode(game: &mut Game, play: CardPlay, index: u64) -> CardPlay {
    let CardPlay::Awaiting(q) = play else {
        panic!("mode")
    };
    assert!(matches!(q.domain, InputDomain::Modes { .. }), "the printed spell offers its two modes");
    game.answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(index)]))
        .unwrap()
}
fn destroy(game: &mut Game) {
    game.resume_internal_card().unwrap();
    resolved(
        game.activate_internal_card("destroyer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn bp05_005_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP05-005"));
    let play = evolve(&mut game, "BP05-005", 1);
    search(&mut game, play, "BP05-005", "field");
}

#[test]
fn bp10_040_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP10-040"));
    let play = enter(&mut game, "BP10-040", 2);
    search(&mut game, play, "BP10-040", "field");
}

#[test]
fn bp10_103_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP10-103"));
    let play = enter(&mut game, "BP10-103", 3);
    search(&mut game, play, "BP10-103", "field");
}

#[test]
fn bp13_062_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP13-062"));
    let play = evolve(&mut game, "BP13-062", 1);
    search(&mut game, play, "BP13-062", "field");
}

#[test]
fn bp18_036_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP18-036"));
    let play = enter(&mut game, "BP18-036", 1);
    search(&mut game, play, "BP18-036", "ex");
}

#[test]
fn bp19_095_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP19-095"));
    let play = enter(&mut game, "BP19-095", 2);
    search(&mut game, play, "BP19-095", "hand");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        14_u8
    );
}

#[test]
fn bp20_007_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP20-007"));
    let play = enter(&mut game, "BP20-007", 2);
    search(&mut game, play, "BP20-007", "field");
}

#[test]
fn bp20_031_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP20-031"));
    let play = enter(&mut game, "BP20-031", 2);
    search(&mut game, play, "BP20-031", "ex");
}

#[test]
fn bp21_020_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("BP21-020"));
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    let play = enter(&mut game, "BP21-020", 2);
    search(&mut game, play, "BP21-020", "field");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before - 6
    );
}

#[test]
fn cp02_048_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("CP02-048"));
    let play = evolve(&mut game, "CP02-048", 1);
    search(&mut game, play, "CP02-048", "hand");
}

#[test]
fn csd02b_004_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("CSD02b-004"));
    let play = enter(&mut game, "CSD02b-004", 1);
    search(&mut game, play, "CSD02b-004", "field");
}

#[test]
fn dsd01b_007_searches_the_printed_cost_type_and_traits() {
    let mut game = fixture(&initial("DSD01b-007"));
    let play = enter(&mut game, "DSD01b-007", 1);
    search(&mut game, play, "DSD01b-007", "field");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        13_u8
    );
}

#[test]
fn bp12_110_heals_then_its_lastword_searches_a_cook() {
    let mut state = initial("BP12-110");
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"destroyer","card":"producer"}]);
    let mut game = fixture(&state);
    resolved(enter(&mut game, "BP12-110", 1));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        13_u8
    );
    destroy(&mut game);
    let play = trigger(&mut game, "BP12-110", 2);
    let play = mode(&mut game, play, 1);
    search(&mut game, play, "BP12-110", "field");
}
#[test]
fn bp19_065_evolution_searches_then_lastword_returns_the_base_to_ex() {
    let mut state = initial("BP19-065");
    state["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"destroyer","card":"producer"}));
    let mut game = fixture(&state);
    let play = evolve(&mut game, "BP19-065", 1);
    search(&mut game, play, "BP19-065", "field");
    destroy(&mut game);
    resolved(trigger(&mut game, "BP19-065", 2));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["source"])
    );
}
#[test]
fn cp04_062_second_mode_searches_only_at_ten_max_pp() {
    let mut game = fixture(&initial("CP04-062"));
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let play = mode(&mut game, play, 2);
    search(&mut game, play, "CP04-062", "field");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8
    );
}
