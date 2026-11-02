#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

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
    ("one", "ordinary", "ニュートラル", "フォロワー", "1", &[]),
    (
        "natural",
        "natural",
        "ニュートラル",
        "アミュレット",
        "1",
        &["自然"],
    ),
    (
        "amulet",
        "ordinary amulet",
        "ニュートラル",
        "アミュレット",
        "1",
        &[],
    ),
    (
        "destroyer",
        "destroyer",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    ("royal", "royal", "ロイヤル", "フォロワー", "1", &[]),
    ("royalspell", "royal spell", "ロイヤル", "スペル", "1", &[]),
    ("retirer", "retirer", "ニュートラル", "フォロワー", "1", &[]),
    ("reviver", "reviver", "ニュートラル", "フォロワー", "1", &[]),
    (
        "tale-follower",
        "tale follower",
        "ニュートラル",
        "フォロワー",
        "5",
        &["童話"],
    ),
    (
        "tale-amulet",
        "tale amulet",
        "ニュートラル",
        "アミュレット",
        "5",
        &["童話"],
    ),
    (
        "costly-tale-follower",
        "costly tale follower",
        "ニュートラル",
        "フォロワー",
        "6",
        &["童話"],
    ),
    (
        "costly-tale-amulet",
        "costly tale amulet",
        "ニュートラル",
        "アミュレット",
        "6",
        &["童話"],
    ),
    (
        "tale-spell",
        "tale spell",
        "ニュートラル",
        "スペル",
        "1",
        &["童話"],
    ),
];
// Synthetic supports isolate filters and evolution forms from unrelated admission gaps.
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if follower {"2"} else {"-"},"hp":if follower {"8"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({});
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        let mut destroy_action = template["abilities"][0].clone();
        destroy_action.as_object_mut().unwrap().remove("evolve");
        destroy_action["activate"] = json!({"cost":{"pp":0_u8},"do":[{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower","other":true}}}]});
        tree["cards"]["destroyer"] = json!({"meta":template["meta"],"abilities":[destroy_action]});
        for (number, steps, targets) in [
            ("retirer", json!([{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower","name":"royal"}}}]), json!([])),
            ("reviver", json!([{"move":{"ref":"chosen","to":"field","result":"returned"}}]), json!([{"bind":"chosen","from":{"who":"self","zone":"cemetery","kind":"follower","class":"ロイヤル"},"n":1_u8}]))
        ] {
            let mut action = template["abilities"][0].clone();
            action.as_object_mut().unwrap().remove("evolve");
            action["activate"] = json!({"cost":{"pp":0_u8},"do":steps});
            if !targets.as_array().unwrap().is_empty() { action["activate"]["targets"] = targets; }
            tree["cards"][number] = json!({"meta":template["meta"],"abilities":[action]});
        }
        (selected, lowerer().lower(&tree).unwrap())
    })
}
fn new_game(initial: &Value) -> Game {
    let (selected, bases) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp07-2",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, bases.get(number).unwrap_or(&empty))
            .unwrap();
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..10_u8)
            .map(|i| json!({"id":format!("deck{i}"),"card":"one"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"one","state":{"acted":true}}]);
    initial
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
#[derive(Clone, Copy)]
struct Decisions {
    mode: usize,
    accept: bool,
    take: usize,
}
const DEFAULT: Decisions = Decisions {
    mode: 0,
    accept: true,
    take: usize::MAX,
};
fn finish(game: &mut Game, step: CardPlay) {
    finish_with(game, step, DEFAULT);
}
fn finish_with(game: &mut Game, mut step: CardPlay, policy: Decisions) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Targets(d) | InputDomain::CostMaterials(d) => {
                let count = usize::try_from(d.cardinality.maximum.unwrap().0)
                    .unwrap()
                    .min(policy.take);
                let ids = d.members.iter().take(count).map(|m| m.handle).collect();
                if matches!(&question.domain, InputDomain::CostMaterials(_)) {
                    Answer::CostMaterials(ids)
                } else {
                    Answer::Targets(ids)
                }
            }
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap().0)
                            .unwrap()
                            .min(policy.take),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Distribution(domain) => {
                let count = domain.members.len();
                Answer::Distribution(
                    domain
                        .members
                        .iter()
                        .enumerate()
                        .map(|(i, m)| DamageShare {
                            handle: m.handle,
                            amount: Nat(if i.checked_add(1).unwrap() == count {
                                domain
                                    .total
                                    .0
                                    .checked_sub(
                                        u64::try_from(count.checked_sub(1).unwrap()).unwrap(),
                                    )
                                    .unwrap()
                            } else {
                                1
                            }),
                        })
                        .collect(),
                )
            }
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(policy.accept),
            InputDomain::Integer(bounds) => {
                assert_eq!(
                    bounds.minimum,
                    Nat(0),
                    "the only integer decision is optional acceptance"
                );
                assert_eq!(
                    bounds.maximum,
                    Some(Nat(1)),
                    "optional effects use a boolean decision"
                );
                Answer::Integer(Nat(u64::from(policy.accept)))
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[policy.mode]]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the printed action has the required resources"
    );
    checks(game);
}
fn play_with(game: &mut Game, policy: Decisions) {
    let step = game.play_internal_card("source", PLAYER).unwrap();
    finish_with(game, step, policy);
}
fn trigger_with(game: &mut Game, policy: Decisions) {
    let step = pending(game);
    finish_with(game, step, policy);
}
fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}
fn copies(card: &str, prefix: &str, count: usize) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("{prefix}{i}"),"card":card}))
            .collect::<Vec<_>>()
    )
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
    assert_eq!(options.len(), 1, "only the tested ability is pending");
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
fn evolution(base: &str, form: &str) -> Value {
    let mut initial = position(base, "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the evolution resource pays the printed PP cost"
    );
    checks(game);
}
fn support(game: &mut Game, number: &str, id: &str) {
    let ability = prepared().1[number].card().abilities[0].node.id;
    let step = game.activate_internal_card(id, ability, PLAYER).unwrap();
    finish(game, step);
}
fn guide_creation(number: &str) {
    for accept in [false, true] {
        for mode in [0_usize, 1] {
            let mut initial = position(number, "hand");
            initial["players"]["P1"]["zones"]["field"] = copies("natural", "field", 1);
            initial["players"]["P1"]["zones"]["ex"] = copies("natural", "ex", 2);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"natural"},{"id":"wrong1","card":"one"},{"id":"wrong2","card":"one"},{"id":"tail","card":"one"}]);
            let mut game = new_game(&initial);
            play(&mut game, "source");
            trigger_with(
                &mut game,
                Decisions {
                    mode,
                    accept,
                    ..DEFAULT
                },
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                named(&state, "field", "ナテラの大樹"),
                usize::from(accept && mode == 0),
                "the field alternative creates exactly one full tree"
            );
            assert_eq!(
                named(&state, "ex", "ナテラの大樹"),
                usize::from(accept && mode == 1),
                "the EX alternative creates exactly one full tree"
            );
            assert_eq!(
                state["P1"]["hand"],
                if accept { json!(["picked"]) } else { json!([]) },
                "three initial Nature cards plus the guide need the new tree to reach five"
            );
            assert_eq!(
                state["P1"]["deck"],
                if accept {
                    json!(["tail", "wrong2", "wrong1"])
                } else {
                    json!(["picked", "wrong1", "wrong2", "tail"])
                },
                "declining tree creation preserves the four-card threshold and original deck"
            );
        }
    }
}
#[test]
fn wasteland_guide_and_pr178_can_decline_or_create_then_count_the_new_tree() {
    for number in ["BP07-107", "PR-178"] {
        guide_creation(number);
        guide_edges(number);
    }
}
#[test]
fn evolved_mecha_goblin_creates_a_product_machine_then_its_lastword_creates_repair_mode_in_ex() {
    for number in ["BP07-111", "BP07-P32"] {
        for full in [false, true] {
            let mut initial = evolution("BP07-110", number);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"destroyer","card":"destroyer"}));
            if full {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .extend(
                        copies("one", "filler", 3)
                            .as_array()
                            .unwrap()
                            .iter()
                            .cloned(),
                    );
                initial["players"]["P1"]["zones"]["ex"] = copies("one", "ex", 5);
            }
            let mut game = new_game(&initial);
            evolve(&mut game);
            trigger(&mut game);
            assert_eq!(
                named(
                    &game.projection(View::Referee).unwrap(),
                    "field",
                    "プロダクトマシーン"
                ),
                usize::from(!full),
                "evolution creates one full Product Machine only when field capacity permits"
            );
            support(&mut game, "destroyer", "destroyer");
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                named(&state, "ex", "リペアモード"),
                usize::from(!full),
                "only the evolved lastword creates a Repair Mode, and EX capacity is respected"
            );
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                "the evolved source retains its physical identity on destruction"
            );
        }
    }
}

fn tree_count_choice(game: &mut Game, optional_progress: CardPlay, chosen: usize, maximum: usize) {
    let CardPlay::Awaiting(optional) = optional_progress else {
        panic!("an awakened up-to count first asks optional acceptance");
    };
    let InputDomain::Integer(bounds) = optional.domain else {
        panic!("up-to zero is represented by an explicit optional decision");
    };
    assert_eq!(bounds.minimum, Nat(0), "the up-to effect may be declined");
    assert_eq!(
        bounds.maximum,
        Some(Nat(1)),
        "optional acceptance is boolean"
    );
    let accepted_progress = game
        .answer_internal_card(
            optional.actor,
            optional.token,
            Answer::Integer(Nat(u64::from(chosen > 0))),
        )
        .unwrap();
    if chosen == 0 {
        finish(game, accepted_progress);
        return;
    }
    let CardPlay::Awaiting(count) = accepted_progress else {
        panic!("accepted creation offers one simultaneous group count");
    };
    let InputDomain::Modes { modes, .. } = &count.domain else {
        panic!("resolution alternatives represent the allowed positive counts");
    };
    assert_eq!(
        modes.len(),
        maximum,
        "every printed positive count is offered"
    );
    let counted_progress = game
        .answer_internal_card(
            count.actor,
            count.token,
            Answer::Modes(vec![modes[chosen.checked_sub(1).unwrap()]]),
        )
        .unwrap();
    finish(game, counted_progress);
}
fn tree_creation(number: &str, maximum: usize) {
    let follower = matches!(number, "BP07-064" | "BP07-P19");
    for awakened in [false, true] {
        let choices = if awakened {
            (0..=maximum).collect::<Vec<_>>()
        } else {
            vec![1]
        };
        for chosen in choices {
            let mut initial = position(number, "hand");
            initial["players"]["P1"]["pp"] = if awakened {
                json!({"max":7_u8,"current":7_u8})
            } else {
                json!({"max":6_u8,"current":6_u8})
            };
            let mut game = new_game(&initial);
            let step = if follower {
                play(&mut game, "source");
                pending(&mut game)
            } else {
                game.play_internal_card("source", PLAYER).unwrap()
            };
            if awakened {
                tree_count_choice(&mut game, step, chosen, maximum);
            } else {
                finish(&mut game, step);
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                named(&state, "field", "ナテラの大樹"),
                chosen,
                "nonawakened creates one while awakened permits every count including zero"
            );
            if follower {
                assert!(
                    state["objects"]["source"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("突進")),
                    "Q1023 grants Rush even when zero trees are chosen"
                );
            }
        }
    }
    for occupied in [4_usize, 5] {
        let mut initial = position(number, "hand");
        initial["players"]["P1"]["zones"]["field"] = copies(
            "one",
            "occupied",
            occupied.checked_sub(usize::from(follower)).unwrap(),
        );
        let mut game = new_game(&initial);
        let step = if follower {
            play(&mut game, "source");
            pending(&mut game)
        } else {
            game.play_internal_card("source", PLAYER).unwrap()
        };
        tree_count_choice(&mut game, step, 2, maximum);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            named(&state, "field", "ナテラの大樹"),
            5_usize.checked_sub(occupied).unwrap(),
            "a two-tree creation respects one or zero remaining field slots"
        );
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            5,
            "the field capacity is never exceeded"
        );
        if follower {
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進")),
                "a full field does not suppress the following Rush grant"
            );
        }
    }
}
#[test]
fn dragon_rancher_preserves_awakened_zero_one_two_and_capacity_before_granting_rush() {
    for number in ["BP07-064", "BP07-P19"] {
        tree_creation(number, 2);
    }
}
#[test]
fn dragon_growth_preserves_awakened_zero_one_two_three_and_field_capacity() {
    for number in ["BP07-068", "BP07-P20"] {
        tree_creation(number, 3);
    }
}
fn storm(game: &Game) -> bool {
    game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!("疾走"))
}
#[test]
fn tsubaki_counts_only_royal_followers_and_updates_storm_when_the_cemetery_changes() {
    for number in ["BP07-021", "BP07-U02", "SD07-010"] {
        for count in [4_usize, 5] {
            for enemy in [false, true] {
                let mut initial = position(number, "hand");
                initial["players"]["P1"]["zones"]["cemetery"] = copies("royal", "grave", count);
                if !enemy {
                    initial["players"]["P2"]["zones"]["field"] = json!([]);
                }
                let mut game = new_game(&initial);
                play(&mut game, "source");
                if enemy {
                    trigger(&mut game);
                } else {
                    assert_eq!(
                        pending(&mut game),
                        CardPlay::Cancelled,
                        "without a legal mandatory target only the fanfare is cancelled"
                    );
                    checks(&mut game);
                    assert!(
                        game.internal_pending_choice().unwrap().is_none(),
                        "the cancelled fanfare leaves no unfinished ability choice"
                    );
                }
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P2"]["field"],
                    if enemy && count == 4 {
                        json!(["enemy"])
                    } else {
                        json!([])
                    },
                    "the printed five-card threshold controls destruction and no enemy does not prevent playing"
                );
                assert_eq!(
                    state["P1"]["field"],
                    json!(["source"]),
                    "the follower enters even without a fanfare target"
                );
            }
        }
        let mut initial = position(number, "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"royal","card":"royal"}),
                json!({"id":"retirer","card":"retirer"}),
                json!({"id":"reviver","card":"reviver"}),
            ]);
        initial["players"]["P1"]["zones"]["cemetery"] = copies("royal", "grave", 9);
        initial["players"]["P1"]["zones"]["cemetery"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"wrongkind","card":"royalspell"}),
                json!({"id":"wrongclass","card":"one"}),
            ]);
        let mut game = new_game(&initial);
        assert!(
            !storm(&game),
            "Royal spells and neutral followers do not count toward ten"
        );
        support(&mut game, "retirer", "retirer");
        assert!(
            storm(&game),
            "a tenth Royal follower entering the cemetery immediately grants Storm"
        );
        support(&mut game, "reviver", "reviver");
        assert!(
            !storm(&game),
            "returning one Royal follower immediately removes Storm"
        );
        let mut attack_initial = position(number, "hand");
        attack_initial["players"]["P1"]["zones"]["cemetery"] = copies("royal", "grave", 10);
        let mut attacker_game = new_game(&attack_initial);
        play(&mut attacker_game, "source");
        trigger(&mut attacker_game);
        assert_eq!(
            attacker_game
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "resolved",
            "Storm allows attacking the leader on the turn of entry"
        );
    }
}
#[test]
fn beautiful_art_distributes_three_or_five_damage_to_zero_through_three_targets() {
    for number in ["BP07-045", "BP07-P14", "PR-468"] {
        let mut empty_initial = position(number, "hand");
        empty_initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut empty = new_game(&empty_initial);
        play_with(&mut empty, Decisions { take: 0, ..DEFAULT });
        assert!(
            empty.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "no enemy followers does not prevent playing this up-to-three spell"
        );
        for condition in [
            "none",
            "base",
            "evolved",
            "opponent",
            "hand",
            "cemetery",
            "wrongname",
        ] {
            for count in 0..=3_usize {
                let mut initial = position(number, "hand");
                initial["players"]["P2"]["zones"]["field"] = copies("one", "enemy", 3);
                let card = if condition == "evolved" {
                    "BP07-040"
                } else if condition == "wrongname" {
                    "one"
                } else {
                    "BP07-039"
                };
                let zone = if matches!(condition, "hand" | "cemetery") {
                    condition
                } else {
                    "field"
                };
                let seat = if condition == "opponent" { "P2" } else { "P1" };
                if condition != "none" {
                    if !initial["players"][seat]["zones"][zone].is_array() {
                        initial["players"][seat]["zones"][zone] = json!([]);
                    }
                    initial["players"][seat]["zones"][zone]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"eleanor","card":card}));
                }
                let mut game = new_game(&initial);
                play_with(
                    &mut game,
                    Decisions {
                        take: count,
                        ..DEFAULT
                    },
                );
                let state = game.projection(View::Referee).unwrap();
                let total = if matches!(condition, "base" | "evolved") {
                    5_u64
                } else {
                    3
                };
                for i in 0..3_usize {
                    let expected = if i >= count {
                        0
                    } else if i.checked_add(1).unwrap() == count {
                        total - u64::try_from(count - 1).unwrap()
                    } else {
                        1
                    };
                    assert_eq!(
                        state["objects"][format!("enemy{i}")]["damage"],
                        expected,
                        "only own field Eleanor changes the total; every chosen target receives at least one"
                    );
                }
                assert!(
                    state["P1"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("source")),
                    "even choosing zero targets resolves the spell"
                );
            }
        }
    }
}
fn assert_private_look(game: &Game, secret_ids: &[&str]) {
    let state = game.projection(View::Referee).unwrap();
    let public = game.projection(View::P2).unwrap();
    let text = public.to_string();
    for id in secret_ids {
        if !state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!(id))
        {
            assert!(
                !text.contains(id),
                "looked cards that were not placed stay absent from the opponent packet"
            );
        }
    }
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the looking player receives a decision"
    );
}
fn bishop_position(mode: usize, eligible: bool, occupied: usize, all_ineligible: bool) -> Value {
    let mut initial = evolution("BP07-098", "BP07-099");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend(
            copies("one", "occupied", occupied.checked_sub(1).unwrap())
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
    let correct = if mode == 0 {
        "tale-follower"
    } else {
        "tale-amulet"
    };
    let other = if all_ineligible {
        "one"
    } else if mode == 0 {
        "tale-amulet"
    } else {
        "tale-follower"
    };
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"secret-eligible","card":if eligible {correct} else {"one"}},
        {"id":"secret-other","card":other}, {"id":"secret-expensive","card":if mode == 0 {"costly-tale-follower"} else {"costly-tale-amulet"}},
        {"id":"secret-spell","card":"tale-spell"}, {"id":"secret-wrongtrait","card":"one"}, {"id":"tail","card":"one"}
    ]);
    initial
}
fn bishop_look(mode: usize, take: bool, eligible: bool, occupied: usize, all_ineligible: bool) {
    let initial = bishop_position(mode, eligible, occupied, all_ineligible);
    let mut game = new_game(&initial);
    evolve(&mut game);
    let mut step = pending(&mut game);
    let secrets = [
        "secret-eligible",
        "secret-other",
        "secret-expensive",
        "secret-spell",
        "secret-wrongtrait",
    ];
    let mut saw_modes = false;
    while let CardPlay::Awaiting(question) = step {
        assert_private_look(&game, &secrets);
        let answer = match &question.domain {
            InputDomain::Modes { modes, .. } => {
                assert_eq!(
                    modes.len(),
                    2,
                    "both public arms remain available independently of the hidden cards"
                );
                saw_modes = true;
                Answer::Modes(vec![modes[mode]])
            }
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    usize::from(eligible),
                    "only the selected kind, Fairy Tale trait and original cost at most five qualify"
                );
                Answer::Search(SearchAnswer::Ungrouped(if take && eligible {
                    vec![domain.cards[0].handle]
                } else {
                    vec![]
                }))
            }
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    usize::from(eligible),
                    "only the selected kind, Fairy Tale trait and original cost at most five qualify"
                );
                Answer::Targets(if take && eligible {
                    vec![domain.members[0].handle]
                } else {
                    vec![]
                })
            }
            InputDomain::Order(members) => {
                Answer::Order(members.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected Bishop decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    checks(&mut game);
    assert_private_look(&game, &secrets);
    assert!(
        saw_modes,
        "the composition exposes its additional public arm choice"
    );
    let state = game.projection(View::Referee).unwrap();
    let placed = take && eligible && occupied < 5;
    let mut rest = secrets
        .iter()
        .filter(|id| !(placed && **id == "secret-eligible"))
        .copied()
        .rev()
        .map(Value::from)
        .collect::<Vec<_>>();
    rest.insert(0, json!("tail"));
    assert_eq!(
        state["P1"]["deck"],
        json!(rest),
        "all unplaced cards return in the chosen bottom order, including a card that cannot fit"
    );
    assert_eq!(
        state["P1"]["field"].as_array().unwrap().len(),
        occupied.checked_add(usize::from(placed)).unwrap(),
        "capacity neither duplicates nor loses a looked card"
    );
}
#[test]
fn black_bishop_selects_either_eligible_kind_orders_the_rest_and_keeps_unplaced_cards_private() {
    for mode in [0_usize, 1] {
        for take in [false, true] {
            for eligible in [false, true] {
                bishop_look(mode, take, eligible, 1, false);
            }
        }
        bishop_look(mode, true, true, 5, false);
        bishop_look(mode, true, false, 1, true);
    }
}

fn guide_edges(number: &str) {
    for mode in [0_usize, 1] {
        for blocked in ["field", "ex", "both"] {
            for accept in [false, true] {
                for take in [0_usize, 1] {
                    let mut initial = position(number, "hand");
                    initial["players"]["P1"]["zones"]["field"] =
                        copies("natural", "naturefield", 1);
                    initial["players"]["P1"]["zones"]["ex"] = copies("natural", "natureex", 2);
                    for zone in ["field", "ex"] {
                        if blocked == zone || blocked == "both" {
                            initial["players"]["P1"]["zones"][zone]
                                .as_array_mut()
                                .unwrap()
                                .extend(copies("one", zone, 3).as_array().unwrap().iter().cloned());
                        }
                    }
                    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"natural"},{"id":"wrong1","card":"one"},{"id":"wrong2","card":"one"},{"id":"tail","card":"one"}]);
                    let mut game = new_game(&initial);
                    play(&mut game, "source");
                    trigger_with(&mut game, Decisions { mode, accept, take });
                    let state = game.projection(View::Referee).unwrap();
                    let created = accept
                        && blocked != "both"
                        && (mode == 0 && blocked != "field" || mode == 1 && blocked != "ex");
                    assert_eq!(
                        named(&state, "field", "ナテラの大樹"),
                        usize::from(created && mode == 0),
                        "a full EX still permits field creation; a full field cannot receive a tree"
                    );
                    assert_eq!(
                        named(&state, "ex", "ナテラの大樹"),
                        usize::from(created && mode == 1),
                        "a full field still permits EX creation; a full EX cannot receive a tree"
                    );
                    assert_eq!(
                        state["P1"]["hand"],
                        if created && take == 1 {
                            json!(["picked"])
                        } else {
                            json!([])
                        },
                        "the post-create Nature threshold is required and the look choice can be declined"
                    );
                    let deck = if !created {
                        json!(["picked", "wrong1", "wrong2", "tail"])
                    } else if take == 1 {
                        json!(["tail", "wrong2", "wrong1"])
                    } else {
                        json!(["tail", "wrong2", "wrong1", "picked"])
                    };
                    assert_eq!(
                        state["P1"]["deck"], deck,
                        "all looked cards that were not taken return in the chosen bottom order"
                    );
                    let public = game.projection(View::P2).unwrap();
                    let known = public["known_cards"].as_array().unwrap();
                    assert_eq!(
                        known.iter().any(|c| c["id"] == "picked"),
                        created && take == 1,
                        "only a card actually revealed and added to hand is disclosed"
                    );
                    assert!(
                        known
                            .iter()
                            .all(|c| c["id"] != "wrong1" && c["id"] != "wrong2"),
                        "the remaining looked cards stay private"
                    );
                }
            }
        }
    }
}
