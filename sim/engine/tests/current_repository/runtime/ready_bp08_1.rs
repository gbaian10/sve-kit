#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
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
const CONTROLS: [SupportCard; 33] = [
    ("one", "ordinary", "ニュートラル", "フォロワー", "1", &[]),
    ("hunter2", "hunter2", "エルフ", "フォロワー", "2", &["狩人"]),
    ("hunter3", "hunter3", "エルフ", "フォロワー", "3", &["狩人"]),
    ("royal2", "royal2", "ロイヤル", "フォロワー", "2", &[]),
    ("royal3", "royal3", "ロイヤル", "フォロワー", "3", &[]),
    ("royalspell", "royalspell", "ロイヤル", "スペル", "1", &[]),
    (
        "commander",
        "commander",
        "ロイヤル",
        "アミュレット",
        "2",
        &["指揮官"],
    ),
    (
        "soldier",
        "soldier",
        "ロイヤル",
        "フォロワー",
        "2",
        &["兵士"],
    ),
    ("elf5", "elf5", "エルフ", "フォロワー", "5", &[]),
    ("elf4", "elf4", "エルフ", "フォロワー", "4", &[]),
    ("elfamulet", "elfamulet", "エルフ", "アミュレット", "5", &[]),
    ("dragon", "dragon", "ドラゴン", "フォロワー", "5", &["竜族"]),
    ("doll", "doll", "エルフ", "フォロワー", "1", &["人形"]),
    (
        "dollamulet",
        "dollamulet",
        "エルフ",
        "アミュレット",
        "1",
        &["人形"],
    ),
    (
        "idol",
        "idol",
        "ウィッチ",
        "アミュレット",
        "1",
        &["アイドル"],
    ),
    ("dead", "dead", "ナイトメア", "フォロワー", "9", &["死者"]),
    (
        "deadamulet",
        "deadamulet",
        "ナイトメア",
        "アミュレット",
        "1",
        &["死者"],
    ),
    ("night5", "night5", "ナイトメア", "フォロワー", "5", &[]),
    ("night6", "night6", "ナイトメア", "フォロワー", "6", &[]),
    ("victoria", "ヴィクトリア", "エルフ", "フォロワー", "1", &[]),
    ("amulet", "amulet", "ニュートラル", "アミュレット", "1", &[]),
    ("tool", "tool", "ニュートラル", "フォロワー", "1", &[]),
    ("cheer", "cheer", "ロイヤル", "フォロワー", "2", &[]),
    (
        "005-base",
        "殺戮の人形・ツヴァイ",
        "エルフ",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "073-base",
        "昏く淀む者・タルタロス",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "107-base",
        "サハクィエル",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    ("sweeper", "sweeper", "ニュートラル", "フォロワー", "1", &[]),
    (
        "dragon3",
        "dragon3",
        "ドラゴン",
        "フォロワー",
        "3",
        &["竜族"],
    ),
    ("strong", "strong", "ニュートラル", "フォロワー", "1", &[]),
    ("zero", "zero", "ニュートラル", "フォロワー", "1", &[]),
    ("booster", "booster", "ニュートラル", "フォロワー", "1", &[]),
    ("zeroer", "zeroer", "ニュートラル", "フォロワー", "1", &[]),
    ("weak", "weak", "ニュートラル", "フォロワー", "1", &[]),
];
// Synthetic supports isolate filters and evolution forms from unrelated admission gaps.
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if number == "weak" {"1"} else if number == "strong" {"5"} else if number == "zero" {"0"} else if follower {"2"} else {"-"},"hp":if follower {"8"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({});
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        for (number, ..) in CONTROLS {
            if number.ends_with("-base") { tree["cards"][number] = template.clone(); }
        }
        let mut tool = template["abilities"][0].clone();
        tool.as_object_mut().unwrap().remove("evolve");
        tool["activate"] = json!({"cost":{"pp":0_u8},"do":[{"damage":{"to":{"who":"self","zone":"field","name":"ヴィクトリア"},"n":1_u8}}]});
        tree["cards"]["tool"] = json!({"meta":template["meta"],"abilities":[tool]});
        let mut cheer = template["abilities"][0].clone();
        cheer.as_object_mut().unwrap().remove("evolve");
        cheer["fanfare"] = json!({"do":[{"life":{"who":"self","delta":1_u8}}]});
        tree["cards"]["cheer"] = json!({"meta":template["meta"],"abilities":[cheer]});
        let mut sweep=template["abilities"][0].clone();
        sweep.as_object_mut().unwrap().remove("evolve");
        sweep["activate"]=json!({"cost":{"pp":0_u8},"do":[{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower","other":true}}}]});
        tree["cards"]["sweeper"]=json!({"meta":template["meta"],"abilities":[sweep]});
        for (number,delta) in [("booster",3_i8),("zeroer",-2)] {
            let mut action=template["abilities"][0].clone();
            action.as_object_mut().unwrap().remove("evolve");
            action["activate"]=json!({"cost":{"pp":0_u8},"do":[{"buff":{"on":{"who":"self","zone":"field","name":"ordinary"},"power":delta}}]});
            tree["cards"][number]=json!({"meta":template["meta"],"abilities":[action]});
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
        "ready-bp08",
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
    initial["players"]["P2"]["zones"]["deck"] = copies("one", "opponent-deck", 3);
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
        let public = game.projection(View::P2).unwrap();
        for secret in ["secret1", "secret2", "secret3", "secret4"] {
            assert!(
                !public["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["id"] == secret),
                "unselected looked cards stay private at every decision"
            );
        }
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
fn evolve(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
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

fn pass_quick(game: &mut Game) {
    for _ in 0..4_u8 {
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step);
    }
    panic!("Quick window did not close");
}
fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pass_quick(game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    checks(game);
}
fn cancel_trigger(game: &mut Game) {
    let step = pending(game);
    assert!(
        matches!(step, CardPlay::Cancelled),
        "only the mandatory-target ability cancels"
    );
    checks(game);
}

fn search_one(game: &mut Game) {
    let CardPlay::Awaiting(question) = pending(game) else {
        panic!("one legal search result")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("deck search")
    };
    assert_eq!(
        domain.cards.len(),
        1,
        "only the printed trait filter qualifies; the ordinary control is rejected"
    );
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    finish(game, step);
}

fn activation(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id
}
#[test]
fn zweis_evolution_protects_only_her_own_victoria_for_this_turn() {
    for number in ["BP08-005", "BP08-P02", "SP01-005", "SP01-SL05"] {
        for present in [false, true] {
            let mut initial = evolution("005-base", number);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"tool","card":"tool"}),
                    json!({"id":"wrong","card":"one"}),
                ]);
            if present {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"victoria","card":"victoria"}));
            }
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"victoria","state":{"acted":true}}]);
            let mut game = new_game(&initial);
            evolve(&mut game, 1);
            if !present {
                cancel_trigger(&mut game);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
                    4_u8,
                    "evolution completes before target cancellation"
                );
                continue;
            }
            let CardPlay::Awaiting(question) = pending(&mut game) else {
                panic!("Victoria must be chosen")
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("name and ownership filter")
            };
            assert_eq!(
                domain.members.len(),
                1,
                "wrong name and opposing Victoria are excluded"
            );
            let step = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            finish(&mut game, step);
            support(&mut game, "tool", "tool");
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"victoria","target":"enemy"}),
            )
            .unwrap();
            pass_quick(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["victoria"]["damage"], 0_u8,
                "both ability and combat damage are prevented"
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"], 2_u8,
                "the opposing Victoria is unprotected"
            );
            end_turn(&mut game);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"enemy","target":"victoria"}),
            )
            .unwrap();
            pass_quick(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["victoria"]["damage"],
                2_u8,
                "the prevention expired with the previous turn"
            );
        }
    }
}
#[test]
fn mitzel_searches_only_original_cost_five_or_more_elf_followers() {
    for eligible in [false, true] {
        let mut initial = evolution("BP08-008", "BP08-009");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"cheap","card":"elf4"},{"id":"amulet","card":"elfamulet"},{"id":"wrong-class","card":"dragon"}]);
        if eligible {
            initial["players"]["P1"]["zones"]["deck"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"found","card":"elf5"}));
        }
        let mut game = new_game(&initial);
        evolve(&mut game, 2);
        let step = pending(&mut game);
        if eligible {
            let CardPlay::Awaiting(question) = step else {
                panic!("one legal search result")
            };
            let InputDomain::Search(domain) = &question.domain else {
                panic!("deck search")
            };
            assert_eq!(
                domain.cards.len(),
                1,
                "kind, class and original cost are all required"
            );
            let searched = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
                )
                .unwrap();
            finish(&mut game, searched);
        } else {
            finish(&mut game, step);
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if eligible {
                json!(["found"])
            } else {
                json!([])
            },
            "only a legal result reaches hand"
        );
        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            3,
            "all rejected cards remain in the shuffled deck"
        );
    }
}

#[test]
fn fanatic_selects_before_testing_three_idol_cards_and_the_condition_covers_healing() {
    for others in [1_usize, 2] {
        let mut initial = position("BP08-050", "hand");
        initial["players"]["P1"]["zones"]["field"] = copies("idol", "idol", others);
        let mut game = new_game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        let applies = others == 2;
        assert_eq!(
            state["P1"]["leader"]["life"],
            if applies { 12_u8 } else { 10 },
            "healing belongs to the conditional paragraph"
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if applies { 18_u8 } else { 20 },
            "damage belongs to the same paragraph"
        );
        assert_eq!(
            state["P2"]["field"],
            if applies { json!([]) } else { json!(["enemy"]) },
            "the source counts as an Idol and Idol amulets also count"
        );
    }
    let mut initial = position("BP08-050", "hand");
    initial["players"]["P1"]["zones"]["field"] = copies("idol", "idol", 2);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = new_game(&initial);
    play(&mut game, "source");
    cancel_trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8,
        "no legal mandatory target cancels the entire fanfare including healing"
    );
}

#[test]
fn arioch_lastword_can_choose_a_qualified_follower_destroyed_simultaneously() {
    let mut initial = position("BP08-080", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"sweeper","card":"sweeper"}),
            json!({"id":"fallen","card":"night5"}),
        ]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"cost-six","card":"night6"},{"id":"wrong-class","card":"royal2"}]);
    let mut game = new_game(&initial);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進")),
        "printed Rush is retained"
    );
    support(&mut game, "sweeper", "sweeper");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["sweeper", "fallen"]),
        "the follower in the same destruction batch is a legal Lastword target under Q1111"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["cost-six", "wrong-class", "source"]),
        "cost six and other classes are excluded"
    );
}
#[test]
fn sahaquiel_grants_rush_and_assail_permanently_only_to_followers_present_on_evolution() {
    for number in ["BP08-107", "BP08-P26"] {
        let mut initial = evolution("107-base", number);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"one"}));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"later","card":"one"}]);
        let mut game = new_game(&initial);
        evolve(&mut game, 1);
        trigger(&mut game);
        play(&mut game, "later");
        for after_turn in [false, true] {
            if after_turn {
                end_turn(&mut game);
            }
            let state = game.projection(View::Referee).unwrap();
            for id in ["source", "old"] {
                for keyword in ["突進", "指定攻撃"] {
                    assert!(
                        state["objects"][id]["keywords"]
                            .as_array()
                            .unwrap()
                            .contains(&json!(keyword)),
                        "the grant includes the evolved source and persists after this turn"
                    );
                }
            }
            assert!(
                !state["objects"]["later"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進")),
                "a later entrant was outside the resolved set under Q1137"
            );
        }
    }
}
#[test]
fn liam_creates_puppets_up_to_ex_capacity_and_rests_to_look_only_after_three_doll_cards() {
    for number in ["BP08-010", "BP08-P03"] {
        for occupied in [0_usize, 4, 5] {
            for cemetery in [2_usize, 3] {
                for take in [0_usize, 1] {
                    let mut initial = position(number, "hand");
                    initial["players"]["P1"]["zones"]["ex"] = copies("one", "ex", occupied);
                    initial["players"]["P1"]["zones"]["cemetery"] =
                        copies("dollamulet", "dollgrave", cemetery);
                    initial["players"]["P1"]["zones"]["cemetery"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"non-doll","card":"one"}));
                    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"dollamulet"},{"id":"secret1","card":"one"},{"id":"secret2","card":"one"},{"id":"tail","card":"one"}]);
                    let mut game = new_game(&initial);
                    play(&mut game, "source");
                    trigger(&mut game);
                    let created_ex = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        named(&created_ex, "ex", "操り人形"),
                        2_usize.min(5_usize.checked_sub(occupied).unwrap()),
                        "token creation respects remaining EX capacity"
                    );
                    let action =
                        game.activate_internal_card("source", activation(number, 2), PLAYER);
                    if cemetery == 2 {
                        assert!(
                            action.is_err(),
                            "non-Doll cards do not satisfy the three-card activation condition"
                        );
                        continue;
                    }
                    finish_with(&mut game, action.unwrap(), Decisions { take, ..DEFAULT });
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        state["objects"]["source"]["acted"], true,
                        "the printed activation cost rests Liam"
                    );
                    assert!(
                        game.activate_internal_card("source", activation(number, 2), PLAYER)
                            .is_err(),
                        "a rested Liam cannot activate again"
                    );
                    assert_eq!(
                        state["P1"]["hand"],
                        if take == 1 {
                            json!(["found"])
                        } else {
                            json!([])
                        },
                        "taking a Doll card is optional and does not require a follower"
                    );
                    assert_eq!(
                        state["P1"]["deck"],
                        if take == 1 {
                            json!(["tail", "secret2", "secret1"])
                        } else {
                            json!(["tail", "secret2", "secret1", "found"])
                        },
                        "all remaining looked cards go to the bottom in the chosen reverse order"
                    );
                    let public = game.projection(View::P2).unwrap();
                    let known = public["known_cards"].as_array().unwrap();
                    assert_eq!(
                        known.iter().any(|c| c["id"] == "found"),
                        take == 1,
                        "only the chosen card is revealed"
                    );
                    for id in ["secret1", "secret2"] {
                        assert!(
                            !known.iter().any(|c| c["id"] == id),
                            "unselected looked cards remain private"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn circle_debuffs_below_zero_and_its_sacrifice_pays_before_reviving_a_qualified_hunter() {
    for number in ["BP08-017", "BP08-P04"] {
        for eligible in [false, true] {
            let mut initial = position(number, "hand");
            initial["players"]["P1"]["zones"]["field"] = copies("one", "old", 4);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"weak","state":{"acted":true}}]);
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"expensive","card":"hunter3"},{"id":"wrong-trait","card":"royal2"}]);
            if eligible {
                initial["players"]["P1"]["zones"]["cemetery"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"found","card":"hunter2"}));
            }
            let mut game = new_game(&initial);
            play(&mut game, "source");
            trigger(&mut game);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["objects"]["enemy"]["power"], -1_i8,
                "Q1064 permits negative power"
            );
            let action = game.activate_internal_card("source", activation(number, 2), PLAYER);
            if !eligible {
                assert!(
                    action.is_err(),
                    "an empty cemetery target domain prevents paying any cost"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "failed target declaration changes neither PP nor the source"
                );
                continue;
            }
            finish(&mut game, action.unwrap());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"], 7_u8,
                "the printed amulet cost two and activation cost one are both paid"
            );
            assert_eq!(
                state["P1"]["field"],
                json!(["old0", "old1", "old2", "old3", "found"]),
                "sacrificing the amulet opens the fifth slot before the hunter returns"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["expensive", "wrong-trait", "source"]),
                "original cost and Hunter filters exclude both controls"
            );
            end_turn(&mut game);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"enemy","target":"P1.leader"}),
            )
            .unwrap();
            pass_quick(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10_u8,
                "a negative-power attack deals no damage"
            );
        }
    }
}
#[test]
fn aether_searches_either_trait_without_a_kind_restriction_and_buffs_allies_only_at_own_end() {
    for number in ["BP08-018", "BP08-SL04", "SD07-005"] {
        for chosen in ["commander", "soldier"] {
            for count in [6_usize, 7] {
                let mut initial = position(number, "hand");
                initial["players"]["P1"]["zones"]["field"] = copies("one", "old", 1);
                initial["players"]["P1"]["zones"]["deck"] =
                    json!([{"id":"found","card":chosen},{"id":"wrong","card":"one"}]);
                initial["players"]["P1"]["zones"]["cemetery"] = copies("royal2", "royal", count);
                initial["players"]["P1"]["zones"]["cemetery"]
                    .as_array_mut()
                    .unwrap()
                    .extend([
                        json!({"id":"spell","card":"royalspell"}),
                        json!({"id":"nonroyal","card":"one"}),
                    ]);
                let mut game = new_game(&initial);
                play(&mut game, "source");
                search_one(&mut game);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["hand"],
                    json!(["found"]),
                    "either Commander or Soldier qualifies, including an amulet"
                );
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap();
                checks(&mut game);
                trigger(&mut game);
                let state = game.projection(View::Referee).unwrap();
                let power = if count == 7 { 3_u8 } else { 2 };
                assert_eq!(
                    state["objects"]["old0"]["power"], power,
                    "the threshold counts only Royal followers but the buff includes non-Royal allies"
                );
                assert_eq!(
                    state["objects"]["source"]["power"], power,
                    "Aether includes herself in the buff"
                );
                assert_eq!(
                    state["objects"]["old0"]["hp"],
                    if count == 7 { 9_u8 } else { 8 },
                    "the same followers gain one health"
                );
            }
        }
        let mut initial = position(number, "field");
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["cemetery"] = copies("royal2", "royal", 7);
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        checks(&mut game);
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the opponent's end phase does not trigger Aether"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            2_u8,
            "no buff occurs in the opponent end phase"
        );
    }
}

#[test]
fn spirit_dragon_puts_only_a_dragon_follower_in_ex_and_discounts_only_it_until_this_turn_ends() {
    for awakened in [false, true] {
        for take in [0_usize, 1] {
            for occupied in [0_usize, 5] {
                let mut initial = position("BP08-061", "hand");
                initial["players"]["P1"]["pp"] = json!({"current":if awakened {10_u8} else {6},"max":if awakened {10_u8} else {6}});
                initial["players"]["P1"]["zones"]["ex"] = copies("one", "ex", occupied);
                initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"dragon3"},{"id":"secret1","card":"one"},{"id":"secret2","card":"dollamulet"},{"id":"secret3","card":"royal2"},{"id":"secret4","card":"doll"},{"id":"tail","card":"one"}]);
                let mut game = new_game(&initial);
                play(&mut game, "source");
                trigger_with(&mut game, Decisions { take, ..DEFAULT });
                let placed = take == 1 && occupied == 0;
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["ex"].as_array().unwrap().len(),
                    occupied.checked_add(usize::from(placed)).unwrap(),
                    "full EX leaves the selected card among the bottomed cards"
                );
                assert_eq!(
                    state["P1"]["deck"],
                    if placed {
                        json!(["tail", "secret4", "secret3", "secret2", "secret1"])
                    } else {
                        json!(["tail", "secret4", "secret3", "secret2", "secret1", "found"])
                    },
                    "all nonmoved looked cards return in the chooser's order"
                );
                let public = game.projection(View::P2).unwrap();
                for id in ["secret1", "secret2", "secret3", "secret4"] {
                    assert!(
                        !public["known_cards"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|c| c["id"] == id),
                        "unselected looked cards stay private"
                    );
                }
                if placed {
                    let before = state["P1"]["pp"]["current"].as_u64().unwrap();
                    play(&mut game, "found");
                    assert_eq!(
                        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                        before.checked_sub(if awakened { 0 } else { 3 }).unwrap(),
                        "only the looked and moved card gets the awakened three-PP discount"
                    );
                }
            }
        }
    }
    for delayed in [false, true] {
        spirit_dragon_discount_scope(delayed);
    }
}

fn spirit_dragon_discount_scope(delayed: bool) {
    let mut initial = position("BP08-061", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"other","card":"dragon3"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"dragon3"},{"id":"secret1","card":"one"},{"id":"secret2","card":"one"},{"id":"secret3","card":"one"},{"id":"secret4","card":"one"},{"id":"tail","card":"one"}]);
    let mut game = new_game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    if delayed {
        end_turn(&mut game);
        end_turn(&mut game);
    }
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    play(&mut game, "other");
    let after_other = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    assert_eq!(
        after_other,
        before.checked_sub(3).unwrap(),
        "an unselected card with the same printed traits receives no discount"
    );
    play(&mut game, "found");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        after_other
            .checked_sub(if delayed { 3 } else { 0 })
            .unwrap(),
        "the selected card costs zero this turn and its printed three next turn"
    );
}

#[test]
fn dione_stands_for_the_first_two_attacks_then_the_quota_resets_next_turn() {
    for number in ["BP08-020", "BP08-SL06", "BP08-U02"] {
        let mut initial = evolution("BP08-019", number);
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
        let mut game = new_game(&initial);
        evolve(&mut game, 6);
        assert!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            "printed Storm permits leader attacks on the turn the base entered"
        );
        for attack in 0..3_u8 {
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            checks(&mut game);
            if attack < 2 {
                trigger(&mut game);
            } else {
                assert!(
                    game.internal_pending_choice().unwrap().is_none(),
                    "the third attack does not exceed the two-trigger quota"
                );
            }
            pass_quick(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                attack == 2,
                "exactly the first two attacks stand Dione"
            );
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            11_u8,
            "Storm permits three leader attacks for three printed power each"
        );
        let fourth = game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        assert_eq!(
            fourth.outcome, "cannot-attack",
            "the rested source cannot attack a fourth time"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"], 11_u8,
            "the rejected fourth attack deals no damage"
        );
        assert_eq!(
            state["objects"]["source"]["acted"], true,
            "the rejected fourth attack leaves Dione rested"
        );
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the rejected fourth attack creates no ability occurrence"
        );
        end_turn(&mut game);
        end_turn(&mut game);
        for attack in 0..3_u8 {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
                )
                .unwrap()
                .outcome,
                "resolved",
                "the next turn permits all three attacks"
            );
            checks(&mut game);
            if attack < 2 {
                trigger(&mut game);
            } else {
                assert!(
                    game.internal_pending_choice().unwrap().is_none(),
                    "the refreshed quota still stops after two occurrences"
                );
            }
            pass_quick(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                attack == 2,
                "the next turn restores exactly two stands"
            );
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            2_u8,
            "three more attacks deal exactly nine damage after the quota resets"
        );
    }
}
