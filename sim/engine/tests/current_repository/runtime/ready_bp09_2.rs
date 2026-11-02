#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use core::iter::once;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
type Control = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const CONTROLS: [Control; 10] = [
    ("plain", "ordinary", "フォロワー", "1", &[]),
    ("onion", "オニオン軍団", "フォロワー", "1", &[]),
    (
        "barzent-form",
        "オニオン軍団の長・バーゼント",
        "フォロワー・エボルヴ",
        "-",
        &[],
    ),
    ("academy", "Academy control", "スペル", "1", &["学院"]),
    ("faith4", "Faith boundary", "フォロワー", "4", &["信仰"]),
    (
        "faith5",
        "Faith over boundary",
        "フォロワー",
        "5",
        &["信仰"],
    ),
    ("wrongtrait", "Wrong trait", "フォロワー", "4", &[]),
    ("faithamulet", "Wrong kind", "アミュレット", "4", &["信仰"]),
    ("amulet", "ordinary amulet", "アミュレット", "1", &[]),
    ("wipe", "external departure", "アミュレット", "1", &[]),
];
// Ordinary same-name controls isolate movement and evolution without claiming other cards' admission.
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static FIXTURE: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, traits) in CONTROLS {
            let follower = kind.starts_with("フォロワー");
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if follower {"2"} else {"-"},"hp":if follower {"9"} else {"-"},"traits":traits,"text":if kind.contains("エボルヴ") {json!("")} else {Value::Null},"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        let mut ability = template["abilities"][0].clone();
        ability.as_object_mut().unwrap().remove("evolve");
        ability["activate"] = json!({"cost":{"pp":0_u8},"do":[{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}]});
        tree["cards"]["wipe"] = json!({"meta":template["meta"],"abilities":[ability]});
        (selected, lowerer().lower(&tree).unwrap())
    })
}
fn build(initial: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp09-second",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, _, kind, ..) in CONTROLS {
        if !kind.contains("エボルヴ") {
            game.load_internal_card(number, programs.get(number).unwrap_or(&empty))
                .unwrap();
        }
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..8_u8)
                .map(|i| json!({"id":format!("{seat}-draw{i}"),"card":"plain"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"plain"}]);
    initial
}
fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
#[derive(Clone, Copy)]
struct Policy {
    take: usize,
    accept: bool,
}
const DEFAULT: Policy = Policy {
    take: usize::MAX,
    accept: true,
};
#[derive(Default)]
struct Trace {
    maxima: Vec<u64>,
    targets: Vec<Vec<String>>,
    searches: Vec<Vec<String>>,
    costs: Vec<Vec<String>>,
    cancelled: bool,
}
fn finish(game: &mut Game, mut play: CardPlay, policy: Policy) -> Trace {
    let mut trace = Trace::default();
    while let CardPlay::Awaiting(question) = play {
        let answer =
            match &question.domain {
                InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                    let maximum = domain.cardinality.maximum.unwrap().0;
                    let cards = domain
                        .members
                        .iter()
                        .filter_map(|m| m.card.clone())
                        .collect::<Vec<_>>();
                    let cost = matches!(&question.domain, InputDomain::CostMaterials(_));
                    if cost {
                        trace.costs.push(cards);
                    } else {
                        trace.maxima.push(maximum);
                        trace.targets.push(cards);
                    }
                    let count = policy.take.min(usize::try_from(maximum).unwrap());
                    let handles = domain
                        .members
                        .iter()
                        .take(count)
                        .map(|m| m.handle)
                        .collect();
                    if cost {
                        Answer::CostMaterials(handles)
                    } else {
                        Answer::Targets(handles)
                    }
                }
                InputDomain::Search(domain) => {
                    trace
                        .searches
                        .push(domain.cards.iter().map(|m| m.card.clone()).collect());
                    Answer::Search(SearchAnswer::Ungrouped(
                        domain
                            .cards
                            .iter()
                            .take(policy.take.min(
                                usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap(),
                            ))
                            .map(|m| m.handle)
                            .collect(),
                    ))
                }
                InputDomain::Order(cards) => {
                    Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
                }
                InputDomain::OptionalCost => Answer::OptionalCost(policy.accept),
                InputDomain::Integer(bounds) => {
                    assert_eq!(
                        bounds.maximum,
                        Some(Nat(1)),
                        "only optional acceptance uses an integer"
                    );
                    Answer::Integer(Nat(u64::from(policy.accept)))
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected decision: {:?}", question.domain),
            };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    trace.cancelled = matches!(play, CardPlay::Cancelled);
    checks(game);
    trace
}
fn pending(game: &mut Game, line: u64) -> CardPlay {
    checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    let option = options.iter().find(|o| o.line == line).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}
fn fire(game: &mut Game, line: u64, policy: Policy) -> Trace {
    let play = pending(game, line);
    finish(game, play, policy)
}
fn support(game: &mut Game, card: &str, id: &str, player: PlayerId) {
    let ability = prepared().1[card].card().abilities[0].node.id;
    let play = game.activate_internal_card(id, ability, player).unwrap();
    assert!(
        !finish(game, play, DEFAULT).cancelled,
        "the external instruction has no material cost"
    );
}
fn play(game: &mut Game) {
    let step = game.play_internal_card("source", PLAYER).unwrap();
    assert!(
        !finish(game, step, DEFAULT).cancelled,
        "the printed card enters"
    );
}
fn pass_quick(game: &mut Game) {
    for _ in 0..4_u8 {
        checks(game);
        if game.internal_card_stage() == Some(ResumeStage::Pending) {
            return;
        }
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, step, DEFAULT);
    }
    panic!("the Quick window closes after both players pass")
}

#[test]
fn barzent_and_printing_look_filter_capacity_order_and_evolve_for_one() {
    for number in ["BP09-042", "BP09-P13"] {
        for (free, take) in [(0_usize, 2_usize), (1, 2), (2, 2), (2, 0), (2, 1)] {
            let mut initial = position(number, "hand");
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..4_usize.checked_sub(free).unwrap())
                    .map(|i| json!({"id":format!("filler{i}"),"card":"plain"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"onion0","card":"onion"},{"id":"onion1","card":"onion"},
                {"id":"other0","card":"plain"},{"id":"onion2","card":"onion"},
                {"id":"other1","card":"plain"},{"id":"tail","card":"plain"}
            ]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"barzent-form"}]);
            let mut game = build(&initial);
            play(&mut game);
            let trace = fire(&mut game, 2, Policy { take, ..DEFAULT });
            assert!(
                !trace.cancelled,
                "the printed Look resolves at any field capacity"
            );
            if free > 0 {
                assert_eq!(
                    trace.maxima.len(),
                    1,
                    "a nonfull field requires an explicit private target choice even when taking none"
                );
            }
            assert!(
                trace
                    .maxima
                    .iter()
                    .all(|n| *n == u64::try_from(free).unwrap()),
                "multi-card Look upper bound matches free slots including zero"
            );
            assert!(
                trace.targets.iter().flatten().all(|c| c == "onion"),
                "only the named cards are selectable"
            );
            let count = take.min(free);
            let current = state(&game);
            let field = current["P1"]["field"].as_array().unwrap();
            assert_eq!(
                field
                    .iter()
                    .filter(|id| current["objects"][id.as_str().unwrap()]["card"] == "onion")
                    .count(),
                count,
                "exactly the fitting selected onions enter"
            );
            let observed = ["onion0", "onion1", "other0", "onion2", "other1"];
            let expected = once(json!("tail"))
                .chain(
                    observed
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i >= count)
                        .rev()
                        .map(|(_, id)| json!(id)),
                )
                .collect::<Vec<_>>();
            assert_eq!(
                current["P1"]["deck"],
                json!(expected),
                "all and only unplaced looked cards go to the chosen bottom order"
            );
            assert_eq!(current["P1"]["pp"]["current"], 7_u8, "playing pays three");
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the real base pays its one PP intrinsic evolution"
            );
            assert_eq!(
                state(&game)["P1"]["pp"]["current"],
                6_u8,
                "evolution consumes exactly one more PP"
            );
        }
    }
}

#[test]
fn tico_all_printings_keep_unselected_top_and_reveal_only_taken_academy() {
    for number in ["BP09-050", "DSD01a-018", "PR-241"] {
        for (card, take) in [("academy", 1_usize), ("academy", 0), ("plain", 1)] {
            let mut initial = position(number, "field");
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"external","card":"wipe"}));
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"top-secret","card":card},{"id":"tail","card":"plain"}]);
            let mut game = build(&initial);
            assert!(
                state(&game)["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護")),
                "every printing has printed Ward"
            );
            support(&mut game, "wipe", "external", PLAYER);
            let trace = fire(&mut game, 2, Policy { take, ..DEFAULT });
            assert!(
                !trace.cancelled,
                "Lastword resolves even without a match or accepting a card"
            );
            let chosen = card == "academy" && take == 1;
            let current = state(&game);
            assert_eq!(
                current["P1"]["hand"],
                if chosen {
                    json!(["top-secret"])
                } else {
                    json!([])
                },
                "only an accepted Academy card enters hand"
            );
            assert_eq!(
                current["P1"]["deck"],
                if chosen {
                    json!(["tail"])
                } else {
                    json!(["top-secret", "tail"])
                },
                "unselected cards stay on top rather than moving to bottom"
            );
            let known = game.projection(View::P2).unwrap()["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == "top-secret");
            assert_eq!(
                known, chosen,
                "only the hand card is revealed to the opponent"
            );
            assert_eq!(
                current["P1"]["cemetery"],
                json!(["source"]),
                "the Lastword follows actual departure"
            );
        }
    }
}

fn temple_case(number: &str, counters: u8) {
    let mut initial = position(number, "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"counters":{"祈りカウンター":counters}});
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend((0..4_u8).map(|i| json!({"id":format!("filler{i}"),"card":"plain"})));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"turn-draw","card":"plain"},{"id":"faith","card":"faith4"},
        {"id":"cost5","card":"faith5"},{"id":"nonfaith","card":"wrongtrait"},
        {"id":"notfollower","card":"faithamulet"},{"id":"tail","card":"plain"}
    ]);
    let mut game = build(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert!(
        !fire(&mut game, 1, DEFAULT).cancelled,
        "own end adds a counter and heals"
    );
    assert_eq!(
        state(&game)["objects"]["source"]["counters"]["祈りカウンター"],
        u64::from(counters).checked_add(1).unwrap(),
        "exactly one Prayer counter is added"
    );
    assert_eq!(
        state(&game)["P1"]["leader"]["life"],
        11_u8,
        "own end heals exactly one"
    );
    pass_quick(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pass_quick(&mut game);
    let trace = fire(&mut game, 2, DEFAULT);
    let enters = counters >= 2;
    let current = state(&game);
    assert_eq!(
        current["P1"]["cemetery"],
        if enters { json!(["source"]) } else { json!([]) },
        "threshold is checked at own main after the end-phase increment"
    );
    if enters {
        assert_eq!(
            trace.searches,
            vec![vec!["faith4".to_owned()]],
            "only original-cost-four Faith follower qualifies, excluding cost five and wrong kind or trait"
        );
        assert!(
            current["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("faith")),
            "self departure frees the slot in a previously full field"
        );
    } else {
        assert!(trace.searches.is_empty(), "two counters do not search");
        assert_eq!(
            current["objects"]["source"]["counters"]["祈りカウンター"], 2_u8,
            "opposing end does not add a Prayer counter"
        );
    }
    assert_eq!(
        current["P1"]["leader"]["life"], 11_u8,
        "opposing end does not heal again"
    );
    assert_eq!(
        current["P1"]["field"].as_array().unwrap().len(),
        5,
        "the full field stays legal on both sides of the threshold"
    );
}
#[test]
fn whitefang_and_printing_gain_prayer_then_sacrifice_before_searching_at_three() {
    for number in ["BP09-095", "BP09-P30"] {
        temple_case(number, 1);
        temple_case(number, 2);
    }
}

fn wings_case(reverse: bool, material: bool, full: bool) {
    let mut initial = position("BP09-096", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    if material {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"offering","card":"amulet"}));
    }
    if full {
        let field = initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap();
        while field.len() < 4 {
            let i = field.len();
            field.push(json!({"id":format!("filler{i}"),"card":"plain"}));
        }
    }
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"oppoffering","card":"amulet"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"white","card":"BP09-099"},{"id":"black","card":"BP09-100"},{"id":"tail","card":"plain"}
    ]);
    let mut game = build(&initial);
    play(&mut game);
    let lines = if reverse { [2_u64, 1] } else { [1_u64, 2] };
    for line in lines {
        let trace = fire(&mut game, line, DEFAULT);
        assert_eq!(
            trace.cancelled,
            line == 2 && !material,
            "only the second Fanfare needs an owned other amulet"
        );
        if line == 2 && material {
            assert_eq!(
                trace.costs,
                vec![vec!["amulet".to_owned()]],
                "source and opposing amulet cannot pay"
            );
        }
    }
    let current = state(&game);
    assert!(
        current["P2"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("oppoffering")),
        "opposing amulet never pays the fee"
    );
    assert_eq!(
        current["P1"]["cemetery"],
        if material {
            json!(["offering"])
        } else {
            json!([])
        },
        "only the other owned amulet is sacrificed"
    );
    let white_enters = !full;
    let black_enters = material;
    assert_eq!(
        current["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("white")),
        white_enters,
        "a full field leaves the white Bird in deck in either Fanfare order"
    );
    assert_eq!(
        current["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("black")),
        black_enters,
        "the paid second Fanfare puts the real black Bird in play"
    );
    assert_eq!(
        current["P1"]["deck"]
            .as_array()
            .unwrap()
            .contains(&json!("white")),
        !white_enters,
        "a Bird that cannot enter remains in the shuffled deck"
    );
    assert_eq!(
        current["P1"]["leader"]["life"], 10_u8,
        "searching a Bird into play does not play its healing Fanfare"
    );
    assert_eq!(
        current["P2"]["leader"]["life"], 20_u8,
        "searching a Bird into play does not play its damage Fanfare"
    );
}
#[test]
fn opposed_wings_choose_fanfare_order_and_obey_payment_and_full_field() {
    for reverse in [false, true] {
        for material in [false, true] {
            wings_case(reverse, material, false);
        }
    }
    wings_case(false, true, true);
    wings_case(true, true, true);
    wings_case(false, false, true);
}
