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
const CONTROLS: [SupportCard; 13] = [
    (
        "ezdia-base",
        "不殺の絶傑・エズディア",
        "エルフ",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "usurp-base",
        "簒奪の使徒",
        "ロイヤル",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "destroy-base",
        "破壊の従者",
        "ウィッチ",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "disdain-base",
        "侮蔑の使徒",
        "ドラゴン",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "hunger-base",
        "飢餓の使徒",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    ("one", "one cost", "ニュートラル", "フォロワー", "1", &[]),
    ("two", "two cost", "ニュートラル", "フォロワー", "2", &[]),
    (
        "three",
        "three cost",
        "ニュートラル",
        "フォロワー",
        "3",
        &[],
    ),
    ("five", "five cost", "ウィッチ", "フォロワー", "5", &[]),
    ("six", "six cost", "ウィッチ", "フォロワー", "6", &[]),
    (
        "idol",
        "idol",
        "ウィッチ",
        "アミュレット",
        "4",
        &["アイドル"],
    ),
    ("omen", "omen", "ウィッチ", "フォロワー", "2", &["絶傑"]),
    ("demon", "demon", "ナイトメア", "フォロワー", "2", &["魔界"]),
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
        let evolution_template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for (number, ..) in CONTROLS {
            if number.ends_with("-base") { tree["cards"][number] = evolution_template.clone(); }
        }
        let base = tree["cards"]["disdain-base"].clone();
        let mut ability = base["abilities"][0].clone();
        ability.as_object_mut().unwrap().remove("evolve");
        ability["activate"] = json!({"cost":{"pp":0_u8},"targets":[{"bind":"victim","from":{"who":"self","zone":"field","kind":"follower","name":"侮蔑の使徒"},"n":1_u8}],"do":[{"damage":{"to":"victim","n":1_u8}}]});
        tree["cards"]["two"] = json!({"meta":base["meta"],"abilities":[ability]});
        (selected, lowerer().lower(&tree).unwrap())
    })
}
fn game(initial: &Value) -> Game {
    let (selected, bases) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp05",
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
        json!([{"id":"enemy","card":"five","state":{"acted":true}}]);
    initial
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(vec![d.members[0].handle]),
            InputDomain::Search(d) => {
                assert_eq!(d.cards.len(), 1, "only the printed filter matches");
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
            }
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "printed action has its resources"
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
        "this scenario has one printed pending ability"
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
    let ability = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
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
        "the matching evolution form is available and its cost is payable"
    );
    checks(game);
}
#[test]
fn ezdia_deals_six_only_to_a_leader_with_at_least_ten_life() {
    for life in [9_u8, 10] {
        let mut initial = evolution("ezdia-base", "BP05-002");
        initial["players"]["P2"]["leader"]["life"] = json!(life);
        let mut game = game(&initial);
        evolve(&mut game);
        activate_card(&mut game, "BP05-002", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if life == 10 { 4_u8 } else { 9 }
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}
#[test]
fn usurpation_apostle_evolution_deals_four_to_the_selected_enemy() {
    let mut game = game(&evolution("usurp-base", "BP05-022"));
    evolve(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
}
#[test]
fn adversity_buffs_both_created_tokens_and_recovers_pp_only_against_an_original_cost_six() {
    for expensive in [false, true] {
        let mut initial = position("BP05-024", "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"old","card":"one"}]);
        initial["players"]["P2"]["zones"]["field"][0]["card"] =
            json!(if expensive { "six" } else { "five" });
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 3);
        assert_eq!(
            state["objects"]["old"]["power"], 2_u8,
            "the older follower is not among the created tokens"
        );
        for id in field.iter().skip(1) {
            let token = &state["objects"][id.as_str().unwrap()];
            assert_eq!(token["power"], if expensive { 4_u8 } else { 1 });
            assert_eq!(token["hp"], if expensive { 4_u8 } else { 1 });
        }
        assert_eq!(
            state["P1"]["pp"]["current"],
            if expensive { 10_u8 } else { 8 },
            "Q693 conditions PP recovery too"
        );
    }
}
#[test]
fn gravity_warrior_buffs_only_an_own_token_follower() {
    let mut initial = position("BP05-032", "hand");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"token","card":"BP05-T05"},{"id":"ordinary","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let CardPlay::Awaiting(q) = pending(&mut game) else {
        panic!("token target")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("targets")
    };
    assert_eq!(d.members.len(), 1);
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
        .unwrap();
    finish(&mut game, step);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["token"]["power"], 4_u8);
    assert_eq!(state["objects"]["token"]["hp"], 5_u8);
    assert_eq!(state["objects"]["ordinary"]["power"], 2_u8);
}
#[test]
fn safira_counts_only_witch_followers_then_pays_five_to_gain_storm() {
    let mut initial = position("BP05-040", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"witch1","card":"five"},{"id":"witch2","card":"six"},{"id":"nonfollower","card":"idol"},{"id":"wrongclass","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let before = game.projection(View::Referee).unwrap();
    trigger(&mut game);
    let buffed = game.projection(View::Referee).unwrap();
    assert_eq!(
        buffed["objects"]["source"]["power"].as_u64().unwrap(),
        before["objects"]["source"]["power"].as_u64().unwrap() + 2
    );
    assert!(
        buffed["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
    activate_card(&mut game, "BP05-040", 3);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
}
fn mechanism_search(game: &mut Game) {
    trigger(game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["ex"],
        json!(["found"]),
        "only the original cost one match enters EX"
    );
    let pp = state["P1"]["pp"]["current"].clone();
    play(game, "found");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        pp,
        "the found original cost one card is free this turn"
    );
}
#[test]
fn ironstaff_fanfare_searches_an_original_one_cost_and_discounts_it_in_ex() {
    let mut initial = position("BP05-042", "hand");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"found","card":"one"},{"id":"wrong","card":"two"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    mechanism_search(&mut game);
}
#[test]
fn ironstaff_evolution_repeats_the_one_cost_search_and_discount() {
    let mut initial = evolution("BP05-042", "BP05-043");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"found","card":"one"},{"id":"wrong","card":"two"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    mechanism_search(&mut game);
}
#[test]
fn destruction_servant_evolution_damages_its_selected_follower_and_leader_even_when_the_follower_dies()
 {
    for hp in [1_u8, 8] {
        let mut initial = evolution("destroy-base", "BP05-049");
        initial["players"]["P2"]["zones"]["field"][0]["state"] =
            json!({"acted":true,"hp":hp,"max_hp":hp});
        let mut game = game(&initial);
        evolve(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
        if hp == 1 {
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        } else {
            assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        }
    }
}
#[test]
fn disdain_apostle_discards_to_find_omens_and_ability_damage_on_its_turn_grants_storm() {
    let mut initial = evolution("disdain-base", "BP05-056");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"damager","card":"two"}));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payment","card":"one"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"a","card":"omen"},{"id":"wrong","card":"one"},{"id":"b","card":"omen"},{"id":"rest1","card":"one"},{"id":"rest2","card":"one"},{"id":"unseen","card":"one"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["a", "b"]));
    assert_eq!(state["P1"]["cemetery"], json!(["payment"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "rest2", "rest1", "wrong"])
    );
    let program = &prepared().1["two"];
    let step = game
        .activate_internal_card("damager", program.card().abilities[0].node.id, PLAYER)
        .unwrap();
    finish(&mut game, step);
    trigger(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["source"]["power"].as_u64().unwrap(),
        state["objects"]["source"]["power"]
            .as_u64()
            .unwrap()
            .checked_add(1)
            .unwrap()
    );
    assert!(
        after["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}
#[test]
fn desire_apostle_pays_life_and_acts_to_revive_a_small_demon_then_heals() {
    let mut initial = position("BP05-072", "field");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"wrong","card":"five"},{"id":"revived","card":"demon"}]);
    let mut game = game(&initial);
    activate_card(&mut game, "BP05-072", 2);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 9_u8);
    assert_eq!(before["P1"]["field"], json!(["source", "revived"]));
    assert_eq!(before["objects"]["source"]["acted"], true);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8
    );
}
#[test]
fn ancient_amplifier_creates_a_mystic_artifact_then_retires_to_buff_only_a_token() {
    let mut initial = position("BP05-102", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ordinary","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    trigger(&mut game);
    activate_card(&mut game, "BP05-102", 2);
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 2);
    let token = &state["objects"][field[1].as_str().unwrap()];
    assert_eq!(token["name"], "ミスティックアーティファクト");
    assert_eq!(token["power"], 4_u8);
    assert_eq!(token["hp"], 4_u8);
    assert_eq!(state["objects"]["ordinary"]["power"], 2_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["hand"], json!(["deck0"]));
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}
#[test]
fn hunger_apostle_can_damage_and_buff_another_own_follower() {
    let mut initial = evolution("hunger-base", "BP05-107");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"one"}));
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 5_u8);
    assert_eq!(state["objects"]["ally"]["damage"], 3_u8);
}
#[test]
fn steel_demolisher_banishes_only_own_ex_to_damage_a_leader_or_follower() {
    for follower in [false, true] {
        let mut initial = position("BP05-116", "hand");
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"payment","card":"one"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"one"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let mut step = pending(&mut game);
        while let CardPlay::Awaiting(q) = step {
            let answer = match &q.domain {
                InputDomain::Targets(d) => {
                    assert_eq!(d.members.len(), 2);
                    let chosen = d
                        .members
                        .iter()
                        .find(|m| m.card.is_some() == follower)
                        .unwrap();
                    Answer::Targets(vec![chosen.handle])
                }
                InputDomain::CostMaterials(d) => {
                    assert_eq!(d.members.len(), 1, "only own EX may pay");
                    assert_eq!(d.members[0].player.seat, 1);
                    Answer::CostMaterials(vec![d.members[0].handle])
                }
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                _ => panic!("the printed target or EX cost"),
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["payment"]));
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P2"]["ex"], json!(["foreign"]));
        assert_eq!(
            state["P2"]["leader"]["life"],
            if follower { 20_u8 } else { 18 }
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if follower { 2_u8 } else { 0 }
        );
    }
}

#[test]
fn desire_apostle_at_one_life_loses_before_its_revival_heal_can_resolve() {
    let mut initial = position("BP05-072", "field");
    initial["players"]["P1"]["leader"]["life"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"revived","card":"demon"}]);
    let mut game = game(&initial);
    activate_card(&mut game, "BP05-072", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 0_u8);
    assert_eq!(state["P1"]["field"], json!(["source", "revived"]));
    assert_eq!(
        state["game"]["ended"], true,
        "Q738 checks defeat before resolving the triggered heal"
    );
    assert_eq!(state["game"]["winner"], "P2");
}
