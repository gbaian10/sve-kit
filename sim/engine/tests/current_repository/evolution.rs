use super::runtime::{game, setup};
use super::*;
use sve_engine::game::{Game, Step, View};

fn initial(card: &str, resource: &str, seat: &str) -> Value {
    let mut initial = setup(seat, seat, [20, 20]);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"][seat]["zones"]["field"] = json!([{"id":"base","card":card}]);
    initial["players"][seat]["zones"]["evolve_deck"] = json!([{"id":"resource","card":resource}]);
    initial["players"][seat]["pp"]["current"] = json!(10_u8);
    initial["players"][seat]["ep"] = json!(1_u8);
    initial["players"][seat]["sep"] = json!(1_u8);
    initial
}

fn evolve(game: &mut Game, pay: &Value) -> Step {
    game.internal_rule_action(&json!({
        "do":"evolve","source":"base","evolve_card":"resource","pay":pay
    }))
    .unwrap()
}

fn selected_game(initial: &Value, card: &str) -> Game {
    let (snapshot, keywords, input) = inputs();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    assert!(
        selected.current.contains_key(card),
        "{card} is formally selected"
    );
    let prototypes = serde_json::to_value(&*selected.prototypes).unwrap();
    assert!(
        prototypes["programs"].get(card).is_none(),
        "{card} has no prototype executor"
    );
    Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "current-runtime",
    )
    .unwrap()
}

fn views(game: &Game) -> Vec<Value> {
    [View::P1, View::P2, View::Referee]
        .map(|view| game.projection(view).unwrap())
        .to_vec()
}

fn check_events(step: &Step, seat: &str, super_evolved: bool) {
    let reveal = step
        .events
        .iter()
        .position(|event| event["kind"] == "公開")
        .unwrap();
    let movement = step
        .events
        .iter()
        .position(|event| event["kind"] == "移動")
        .unwrap();
    let evolution = step
        .events
        .iter()
        .position(|event| event["kind"] == "進化")
        .unwrap();
    assert!(
        reveal < movement && movement < evolution,
        "the resource is revealed before movement and evolution"
    );
    assert_eq!(
        step.events[reveal]["object"], "resource",
        "the selected resource is revealed"
    );
    assert_eq!(
        step.events[reveal]["to"], "all",
        "both players see the revealed resource"
    );
    assert_eq!(
        step.events[movement]["object"], "resource",
        "the resource itself moves"
    );
    assert_eq!(
        step.events[movement]["from"],
        format!("{seat}.evolve_deck"),
        "the controller supplies its evolve deck resource"
    );
    assert_eq!(
        step.events[movement]["to"],
        format!("{seat}.evolution"),
        "the resource enters the evolution zone"
    );
    assert_eq!(
        step.events[evolution]["object"], "base",
        "the field object is the evolution subject"
    );
    assert_eq!(
        step.events[evolution]["with"], "resource",
        "the event links the selected resource"
    );
    assert_eq!(
        step.events
            .iter()
            .filter(|event| event["kind"] == "超進化")
            .count(),
        usize::from(super_evolved),
        "only additional SEP produces a super evolution event"
    );
}

fn check_views(game: &Game, seat: &str, base: &str, resource: &str, stats: u8, remaining: [u8; 3]) {
    for state in views(game) {
        assert_eq!(
            state[seat]["evolve_deck"],
            json!([]),
            "the resource leaves its original zone"
        );
        assert_eq!(
            state[seat]["evolution"],
            json!(["resource"]),
            "the public evolution zone contains the resource"
        );
        assert_eq!(
            state["objects"]["base"]["card"], base,
            "the base keeps its permanent card identity"
        );
        assert_eq!(
            state["objects"]["base"]["evolved_with"], "resource",
            "the base retains the resource association"
        );
        assert_eq!(
            state["objects"]["resource"]["card"], resource,
            "the linked object is the selected same-name Evolve print"
        );
        assert_eq!(
            state["objects"]["base"]["name"], state["objects"]["resource"]["name"],
            "the Evolve print has the same printed name"
        );
        assert_eq!(
            state["objects"]["base"]["generation"], 0_u8,
            "evolution does not move the base"
        );
        assert_eq!(
            state["objects"]["resource"]["generation"], 1_u8,
            "the resource movement creates a new generation"
        );
        assert_eq!(
            state["objects"]["base"]["face"], 0_u8,
            "the base uses the selected evolution face"
        );
        assert_eq!(
            state["objects"]["resource"]["face"], 0_u8,
            "the event resource exposes the same face"
        );
        assert_eq!(
            state["objects"]["base"]["power"], stats,
            "attack follows the evolution print and SEP bonus"
        );
        assert_eq!(
            state["objects"]["base"]["hp"],
            stats.saturating_add(u8::from(base == "BP01-174")),
            "life follows the evolution print (BP01-175 is printed 5/6) and SEP bonus"
        );
        assert_eq!(
            state[seat]["pp"]["current"], remaining[0],
            "only the declared PP is spent"
        );
        assert_eq!(
            state[seat]["ep"], remaining[1],
            "EP substitutes at most one PP"
        );
        assert_eq!(
            state[seat]["sep"], remaining[2],
            "SEP is an additional payment"
        );
    }
}

type EvolutionCase = (&'static str, &'static str, Value, u8, [u8; 3]);

/// Every E-B7 permanent card, its Evolve print and the printed evolution PP cost.
const B7_CARDS: [(&str, &str, u8); 9] = [
    ("BP01-110", "BP01-111", 1),
    ("BP01-171", "BP01-172", 4),
    ("BP01-174", "BP01-175", 2),
    ("BP03-115", "BP03-116", 0),
    ("BP04-010", "BP04-011", 1),
    ("BP17-077", "BP17-078", 1),
    ("BP18-058", "BP18-059", 1),
    ("BP19-001", "BP19-002", 1),
    ("BP19-050", "BP19-051", 1),
];

fn run_cases(cases: &[EvolutionCase]) {
    for seat in ["P1", "P2"] {
        for (base, resource, pay, stats, remaining) in cases {
            let base = *base;
            let resource = *resource;
            let mut game = selected_game(&initial(base, resource, seat), base);
            let opponent_view = if seat == "P1" { View::P2 } else { View::P1 };
            let before_public = game.projection(opponent_view).unwrap();
            assert!(
                before_public["objects"].get("resource").is_none(),
                "the opponent cannot identify the hidden resource object"
            );
            assert!(
                !before_public["knowledge"]["identifiable"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("resource")),
                "the opponent cannot track the hidden resource id"
            );
            let step = evolve(&mut game, pay);
            assert_eq!(step.outcome, "resolved", "{base} evolution resolves");
            check_events(&step, seat, pay["sep"] == 1_u8);
            check_views(&game, seat, base, resource, *stats, *remaining);
            // Printed abilities are not authored here: later actions stop, never skip them.
            assert_eq!(
                game.legal().unwrap(),
                Vec::<Value>::new(),
                "{base} resource ability remains pending, so no further action is legal"
            );
            assert!(
                matches!(game.internal_pending_choice(), Err(EngineFailure::Unsupported(reason))
                if reason.contains("printed abilities are not admitted")),
                "{base} resource printed ability stops at the unsupported boundary"
            );
            assert!(
                matches!(
                    game.internal_rule_action(&json!({"do":"end-phase"})),
                    Err(EngineFailure::Unsupported(_))
                ),
                "{base} cannot continue while its resource ability is unadmitted"
            );
        }
    }
}

#[test]
fn e_b7_001_current_evolutions_use_the_accepted_resource_path() {
    run_cases(&[
        (
            "BP01-110",
            "BP01-111",
            json!({"pp":0_u8,"ep":1_u8}),
            3,
            [10, 0, 1],
        ),
        ("BP01-110", "BP01-111", json!({"pp":1_u8}), 3, [9, 1, 1]),
        ("BP01-171", "BP01-172", json!({"pp":4_u8}), 4, [6, 1, 1]),
        (
            "BP01-171",
            "BP01-172",
            json!({"pp":3_u8,"ep":1_u8}),
            4,
            [7, 0, 1],
        ),
        (
            "BP01-171",
            "BP01-172",
            json!({"pp":4_u8,"sep":1_u8}),
            5,
            [6, 1, 0],
        ),
        ("BP01-174", "BP01-175", json!({"pp":2_u8}), 5, [8, 1, 1]),
        (
            "BP01-174",
            "BP01-175",
            json!({"pp":1_u8,"ep":1_u8}),
            5,
            [9, 0, 1],
        ),
        (
            "BP01-174",
            "BP01-175",
            json!({"pp":2_u8,"sep":1_u8}),
            6,
            [8, 1, 0],
        ),
    ]);
}

#[test]
fn e_b7_002_current_evolutions_use_the_accepted_resource_path() {
    // CR 5.16.1.1.1-.3 and 12.2.1-.4 govern the resource face. Q2064 requires
    // applicable printed abilities to be processed; Q2065 permits a zero-cost super evolution.
    run_cases(&[
        ("BP03-115", "BP03-116", json!({"pp":0_u8}), 4, [10, 1, 1]),
        (
            "BP03-115",
            "BP03-116",
            json!({"pp":0_u8,"sep":1_u8}),
            5,
            [10, 1, 0],
        ),
        ("BP04-010", "BP04-011", json!({"pp":1_u8}), 2, [9, 1, 1]),
        (
            "BP04-010",
            "BP04-011",
            json!({"pp":0_u8,"ep":1_u8}),
            2,
            [10, 0, 1],
        ),
        (
            "BP04-010",
            "BP04-011",
            json!({"pp":1_u8,"sep":1_u8}),
            3,
            [9, 1, 0],
        ),
        ("BP17-077", "BP17-078", json!({"pp":1_u8}), 2, [9, 1, 1]),
        (
            "BP17-077",
            "BP17-078",
            json!({"pp":0_u8,"ep":1_u8}),
            2,
            [10, 0, 1],
        ),
        (
            "BP17-077",
            "BP17-078",
            json!({"pp":1_u8,"sep":1_u8}),
            3,
            [9, 1, 0],
        ),
    ]);
}

#[test]
fn each_b7_evolution_rejects_a_missing_current_ability() {
    for (base, resource, cost) in B7_CARDS {
        let (snapshot, keywords, mut input) = inputs();
        input["authors"]
            .as_array_mut()
            .unwrap()
            .retain(|author| author["input"]["card_no"] != base);
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        assert!(
            !selected.current.contains_key(base),
            "{base} is not accepted"
        );
        let mut game = Game::from_selected_effects(
            selected,
            &initial(base, resource, "P1"),
            &Value::Null,
            &Value::Null,
            "current-runtime",
        )
        .unwrap();
        let before = game.legal().unwrap_err();
        assert!(
            matches!(&before, EngineFailure::Unsupported(_)),
            "the missing current program makes the context unsupported"
        );
        let error = game
            .internal_rule_action(&json!({
                "do":"evolve","source":"base","evolve_card":"resource",
                "pay":{"pp":cost,"ep":0_u8}
            }))
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::Unsupported(_)),
            "{base} must not evolve without its accepted current ability: {error:?}"
        );
        let after = game.legal().unwrap_err();
        assert!(matches!(after, EngineFailure::Unsupported(_)));
        assert_eq!(
            format!("{before:?}"),
            format!("{after:?}"),
            "{base} rejection preserves the unsupported context"
        );
    }
}

#[test]
fn each_b7_evolution_rejects_wrong_source_owner_and_missing_payment_atomically() {
    for (base, resource, cost) in B7_CARDS {
        for invalid in 0_u8..3 {
            // A zero printed cost cannot be underpaid.
            if invalid == 2 && cost == 0 {
                continue;
            }
            let mut initial = initial(base, resource, "P1");
            let mut source = "base";
            // Always the exact printed cost, so only the injected defect can reject.
            let pay = json!({"pp":cost});
            match invalid {
                0 => {
                    initial["players"]["P1"]["zones"]["evolve_deck"][0]["owner"] = json!("P2");
                }
                1 => source = "not-the-field-base",
                _ => {
                    initial["players"]["P1"]["pp"]["current"] = json!(cost.saturating_sub(1));
                    initial["players"]["P1"]["ep"] = json!(0_u8);
                }
            }
            let mut game = game(&initial);
            let before = views(&game);
            let result = game.internal_rule_action(&json!({
                "do":"evolve",
                "source":source,
                "evolve_card":"resource",
                "pay":pay
            }));
            if invalid == 1 {
                assert!(result.is_err(), "{base}, case {invalid}");
            } else {
                let result = result.unwrap();
                assert_eq!(result.outcome, "cannot-evolve", "{base}, case {invalid}");
                assert!(result.events.is_empty(), "{base}, case {invalid}");
            }
            assert_eq!(views(&game), before, "{base}, case {invalid}");
        }
    }
}

#[test]
fn printed_four_and_two_pp_costs_cannot_be_replaced_by_two_ep() {
    // CR 12.2.3 permits one EP to replace one PP for evolution.
    for (base, resource) in [("BP01-171", "BP01-172"), ("BP01-174", "BP01-175")] {
        let mut initial = initial(base, resource, "P1");
        initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
        initial["players"]["P1"]["ep"] = json!(2_u8);
        let mut game = game(&initial);
        let before = views(&game);
        let result = game
            .internal_rule_action(&json!({
                "do":"evolve",
                "source":"base",
                "evolve_card":"resource",
                "pay":{"pp":0_u8,"ep":2_u8}
            }))
            .unwrap();
        assert_eq!(result.outcome, "cannot-evolve", "{base}");
        assert!(result.events.is_empty(), "{base}");
        assert_eq!(views(&game), before, "{base}");
    }
}

#[test]
fn current_evolution_rejects_wrong_owner_name_and_unpayable_cost_atomically() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        for case in 0_u8..8 {
            let mut initial = initial("BP01-110", "BP01-111", seat);
            let mut pay = json!({"pp":0_u8,"ep":1_u8});
            match case {
                0 => initial["players"][seat]["zones"]["evolve_deck"][0]["owner"] = json!(other),
                1 => {
                    initial["players"][seat]["zones"]["evolve_deck"][0]["card"] = json!("BP03-116");
                }
                2 => initial["players"][seat]["ep"] = json!(0_u8),
                3 => {
                    initial["players"][seat]["pp"]["current"] = json!(0_u8);
                    pay = json!({"pp":1_u8});
                }
                4 => pay = json!({"pp":0_u8,"ep":2_u8}),
                5 => {
                    initial["players"][seat]["sep"] = json!(0_u8);
                    pay["sep"] = json!(1_u8);
                }
                6 => pay = json!({"pp":0_u8,"ep":0_u8,"sep":1_u8}),
                _ => {
                    initial["players"][seat]["zones"]["evolve_deck"][0]["card"] = json!("BP03-115");
                }
            }
            let mut game = game(&initial);
            let before = views(&game);
            let step = evolve(&mut game, &pay);
            assert_eq!(step.outcome, "cannot-evolve", "case {case}");
            assert!(step.events.is_empty(), "a rejection produces no events");
            assert_eq!(
                views(&game),
                before,
                "case {case} leaves all views unchanged"
            );
        }
    }
}

#[test]
fn evolution_resource_role_cannot_admit_an_ordinary_unloaded_card() {
    // A same-number card outside the evolve deck gets no role; a vanilla one reaches the role check itself.
    for (card, reason) in [
        ("BP03-116", "prototype abilities"),
        ("BP01-042", "unloaded whole card"),
    ] {
        let mut initial = initial("BP01-110", "BP01-111", "P1");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"unloaded","card":card}]);
        let mut game = game(&initial);
        let error = game
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap_err();
        assert!(
            matches!(&error, EngineFailure::Unsupported(text) if text.contains(reason)),
            "{card}: {error:?}"
        );
    }
}

/// Synthetic resource faces (a copy of BP01-111 under a new number) probe the ability-free rule.
fn synthetic_game(text: &Value, sections: &Value) -> Game {
    let (snapshot, keywords, input) = inputs();
    let mut lines = snapshot.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut card = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|card| card["number"] == "BP01-111")
        .unwrap();
    card["number"] = json!("ZZ00-111");
    card["faces"][0]["text"] = text.clone();
    card["faces"][0]["sections"] = sections.clone();
    lines.push(card.to_string());
    let selected = Catalog::from_current_selection(&lines.join("\n"), &keywords, &input).unwrap();
    Game::from_selected_effects(
        selected,
        &initial("BP01-110", "ZZ00-111", "P1"),
        &Value::Null,
        &Value::Null,
        "current-runtime",
    )
    .unwrap()
}

#[test]
fn only_a_known_ability_free_resource_face_lets_play_continue() {
    for (text, sections, stops) in [
        (json!(""), json!([]), false),
        (Value::Null, json!([]), true),
        (json!(""), Value::Null, true),
        (json!(""), json!([{"title":"x","text":"y"}]), true),
    ] {
        let mut game = synthetic_game(&text, &sections);
        evolve(&mut game, &json!({"pp":0_u8,"ep":1_u8}));
        let choice = game.internal_pending_choice();
        if stops {
            assert!(matches!(choice, Err(EngineFailure::Unsupported(reason))
                if reason == "internal D1 does not support evolution resource printed abilities are not admitted"));
        } else {
            assert!(choice.unwrap().is_none());
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
        }
    }
}

#[test]
fn zero_cost_and_early_super_evolution_payments_reject_atomically() {
    // CR 12.2.2/.4 and Q2065-Q2066: SEP is additional; a zero cost remains zero.
    for seat in ["P1", "P2"] {
        for case in 0_u8..3 {
            let mut initial = initial("BP03-115", "BP03-116", seat);
            let pay = match case {
                0 => json!({"pp":1_u8,"sep":1_u8}),
                1 => json!({"ep":1_u8,"sep":1_u8}),
                _ => {
                    initial["turn"]["elapsed_turns"][seat] =
                        json!(if seat == "P1" { 6_u8 } else { 5_u8 });
                    json!({"sep":1_u8})
                }
            };
            let mut game = game(&initial);
            let before = views(&game);
            let result = evolve(&mut game, &pay);
            assert_eq!(result.outcome, "cannot-evolve");
            assert_eq!(result.events, Vec::<Value>::new());
            assert_eq!(views(&game), before);
        }
    }
}
