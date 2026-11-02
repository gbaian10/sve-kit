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
        "lalamia-base",
        "音速の機構・ララミア",
        "ロイヤル",
        "フォロワー",
        "1",
        &[],
    ),
    ("one", "ordinary", "ニュートラル", "フォロワー", "1", &[]),
    ("two", "hunter", "エルフ", "フォロワー", "2", &["狩人"]),
    (
        "three",
        "expensive hunter",
        "エルフ",
        "フォロワー",
        "3",
        &["狩人"],
    ),
    (
        "monkey",
        "ハンマーモンキー",
        "エルフ",
        "フォロワー",
        "2",
        &[],
    ),
    (
        "fairy",
        "フェアリー",
        "エルフ",
        "フォロワー",
        "1",
        &["妖精"],
    ),
    ("dragon", "dragon", "ドラゴン", "フォロワー", "2", &[]),
    (
        "golem",
        "golem",
        "ウィッチ",
        "フォロワー",
        "2",
        &["ゴーレム"],
    ),
    ("spell", "spell", "ウィッチ", "スペル", "1", &[]),
    ("reviver", "reviver", "ニュートラル", "フォロワー", "1", &[]),
    (
        "destroyer",
        "destroyer",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    ("hurter", "hurter", "ニュートラル", "フォロワー", "1", &[]),
    ("amulet", "amulet", "ニュートラル", "アミュレット", "1", &[]),
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
        let ability = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for (number, ..) in CONTROLS {
            if number.ends_with("-base") { tree["cards"][number] = ability.clone(); }
        }
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        for (number, steps, targets) in [
            ("reviver", json!([{"move":{"ref":"subject","to":"field","result":"returned"}}]),json!([{"bind":"subject","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1_u8}])),
            ("destroyer",json!([{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower","name":"フェアリードラゴン"}}}]),json!([])),
            ("hurter",json!([{"life":{"who":"self","delta":-1_i8}}]),json!([])),
        ] {
            let mut action = template["abilities"][0].clone();
            action.as_object_mut().unwrap().remove("evolve");
            action["activate"] = json!({"cost":{"pp":0_u8},"do":steps});
            if !targets.as_array().unwrap().is_empty() {
                action["activate"]["targets"] = targets;
            }
            tree["cards"][number] = json!({"meta":template["meta"],"abilities":[action]});
        }
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
        "ready-bp06",
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
            InputDomain::TokenCapacity { choices } => {
                assert_eq!(
                    choices.len(),
                    2,
                    "one remaining EX slot offers each printed token"
                );
                Answer::TokenCapacity(0)
            }
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
        "the evolution resource pays the printed PP cost"
    );
    checks(game);
}
fn support(game: &mut Game, number: &str, id: &str) {
    let ability = prepared().1[number].card().abilities[0].node.id;
    let step = game.activate_internal_card(id, ability, PLAYER).unwrap();
    finish(game, step);
}
#[test]
fn patriarch_evolves_when_returned_by_an_ability_but_not_when_played() {
    let mut normal = position("BP06-005", "hand");
    normal["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-006"}]);
    let mut played = game(&normal);
    play(&mut played, "source");
    trigger(&mut played);
    assert_eq!(
        played.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source"]),
        "ordinary play does not satisfy ability entry"
    );
    let ordinary = played.projection(View::Referee).unwrap();
    assert_eq!(
        ordinary["objects"]["source"]["evolved_with"],
        Value::Null,
        "ordinary play leaves the base unevolved"
    );
    assert_eq!(
        ordinary["P1"]["evolve_deck"],
        json!(["form"]),
        "ordinary play does not consume an evolution resource"
    );
    assert_eq!(
        ordinary["objects"]["source"]["power"], 3_u8,
        "ordinary play retains the printed base power"
    );
    let mut initial = position("reviver", "field");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":"BP06-005"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-006"}]);
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    let mut revived = game(&initial);
    support(&mut revived, "reviver", "source");
    trigger(&mut revived);
    assert_eq!(
        revived.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source", "returned"]),
        "ability entry preserves the returned object"
    );
    let state = revived.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["returned"]["evolved_with"], "form",
        "ability entry consumes the matching evolution resource"
    );
    assert_eq!(
        state["objects"]["returned"]["power"], 4_u8,
        "the returned object has evolved power"
    );
    assert_eq!(
        state["objects"]["returned"]["hp"], 4_u8,
        "the returned object has evolved life"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 0_u8,
        "effect evolution pays no PP"
    );
}
#[test]
fn evolved_patriarch_returns_only_two_cost_hunters() {
    let mut initial = evolution("BP06-005", "BP06-006");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"costly","card":"three"},{"id":"wrong","card":"one"},{"id":"a","card":"two"},{"id":"b","card":"two"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution costs two PP"
    );
    checks(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "a", "b"]),
        "both eligible hunters return"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["costly", "wrong"]),
        "wrong cost and trait stay in the cemetery"
    );
}
#[test]
fn fairy_dragon_counts_fairy_tokens_in_both_zones_and_offers_lastword_capacity_choice() {
    let mut initial = position("BP06-007", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"field-fairy","card":"BP01-T03"},{"id":"destroyer","card":"destroyer"},{"id":"non-token-fairy","card":"fairy"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-fairy","card":"BP01-T03"},{"id":"ordinary","card":"one"},{"id":"other1","card":"one"},{"id":"other2","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["source"]["power"], 2_u8,
        "only the two Fairy tokens contribute"
    );
    support(&mut game, "destroyer", "destroyer");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["ex"].as_array().unwrap().len(),
        5,
        "only the remaining EX slot is filled"
    );
    let new_id = state["P1"]["ex"][4].as_str().unwrap();
    let name = state["objects"][new_id]["name"].as_str().unwrap();
    assert!(
        ["フェアリーウィスプ", "フェアリー"].contains(&name),
        "the controller receives one of the two printed tokens"
    );
}
#[test]
fn hammer_monkey_pays_two_to_find_a_named_follower() {
    let mut initial = position("BP06-014", "hand");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"found","card":"monkey"},{"id":"wrong","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "found"]),
        "search uses the printed name"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 5_u8,
        "printed play cost plus the optional two PP are paid"
    );
}
#[test]
fn blitz_elf_discount_counts_only_fairies_on_its_own_field() {
    let mut initial = position("BP06-016", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy1","card":"fairy"},{"id":"fairy2","card":"fairy"},{"id":"wrong","card":"one"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"fairy"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 7_u8,
        "two own field Fairies reduce the printed five PP to three"
    );
    for keyword in ["突進", "指定攻撃"] {
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!(keyword)),
            "both printed keywords remain"
        );
    }
}
#[test]
fn evolved_lalamia_pays_three_and_counts_other_followers() {
    let mut initial = evolution("lalamia-base", "BP06-020");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"a","card":"one"}),
            json!({"id":"b","card":"one"}),
            json!({"id":"wrong","card":"amulet"}),
        ]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["source"]["power"], 6_u8,
        "only two other followers contribute"
    );
    assert_eq!(
        state["objects"]["source"]["hp"], 5_u8,
        "both statistics use the same field count"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 6_u8,
        "evolution and optional buff are separate payments"
    );
}
#[test]
fn inferno_reduces_its_cost_at_spellchain_ten_and_damages_enemy_field_and_leader() {
    for graves in [9_u8, 10] {
        let mut initial = position("BP06-047", "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..graves)
                .map(|i| json!({"id":format!("grave{i}"),"card":"spell"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"], 17_u8,
            "all opposing leaders take three"
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"], 6_u8,
            "enemy followers take six"
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            if graves == 10 { 5_u8 } else { 3 },
            "the resolving spell is not counted toward Spellchain payment"
        );
    }
}
#[test]
fn garyu_draws_and_grants_ward_only_to_other_own_dragon_followers() {
    let mut initial = position("BP06-054", "hand");
    initial["players"]["P2"]["zones"]["field"][0]["card"] = json!("dragon");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"dragon","card":"dragon"},{"id":"neutral","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["deck0"]), "fanfare draws one");
    assert!(
        state["objects"]["dragon"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護")),
        "the other Dragon gains Ward"
    );
    for id in ["source", "neutral", "enemy"] {
        assert!(
            !state["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "self, wrong class and opponents do not gain Ward"
        );
    }
}
#[test]
fn frenzied_fur_triggers_only_once_per_turn_and_can_retire_to_heal_at_ten_life() {
    let mut initial = position("BP06-089", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"ally","card":"one"}),
            json!({"id":"hurter","card":"hurter"}),
        ]);
    let mut game = game(&initial);
    support(&mut game, "hurter", "hurter");
    trigger(&mut game);
    let first = game.projection(View::Referee).unwrap();
    assert_eq!(
        first["objects"]["ally"]["power"], 3_u8,
        "first life decrease buffs a chosen follower"
    );
    support(&mut game, "hurter", "hurter");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "second decrease does not create another trigger this turn"
    );
    activate_card(&mut game, "BP06-089", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 10_u8,
        "retirement heals two after two life payments"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["source"]),
        "the amulet is paid as a cost"
    );
}
fn enter_end(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    checks(game);
}
#[test]
fn aragavy_fanfare_pays_five_to_damage_other_followers_and_gain_stats_and_storm() {
    let mut initial = position("BP06-075", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"one"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    for id in ["ally", "enemy"] {
        assert_eq!(
            state["objects"][id]["damage"], 5_u8,
            "both sides' other followers take five"
        );
    }
    assert_eq!(
        state["objects"]["source"]["damage"], 0_u8,
        "the source is excluded from its own damage"
    );
    assert_eq!(
        state["objects"]["source"]["power"], 6_u8,
        "fanfare adds two power to the source"
    );
    assert_eq!(
        state["objects"]["source"]["hp"], 5_u8,
        "fanfare adds two life to the source"
    );
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走")),
        "the source gains Storm"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 2_u8,
        "play and fanfare together cost eight PP"
    );
}
#[test]
fn aragavy_end_target_is_declared_before_the_crimson_condition_resolves() {
    for crimson in [false, true] {
        let mut initial = position("BP06-075", "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"hurter","card":"hurter"}));
        let mut game = game(&initial);
        if crimson {
            support(&mut game, "hurter", "hurter");
        }
        enter_end(&mut game);
        let CardPlay::Awaiting(question) = pending(&mut game) else {
            panic!("formal end target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("leader or follower target");
        };
        let leader = domain
            .members
            .iter()
            .find(|member| member.card.is_none())
            .unwrap();
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![leader.handle]),
            )
            .unwrap();
        finish(&mut game, step);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if crimson { 17_u8 } else { 20 },
            "only an earlier life decrease enables the printed end damage"
        );
    }
}
#[test]
fn karla_optional_end_target_preserves_damage_and_draw_when_no_enemy_follower_exists() {
    for empty in [false, true] {
        for pp in [1_u8, 2, 4, 6] {
            let mut initial = position("BP06-091", "field");
            initial["players"]["P1"]["pp"]["current"] = json!(pp);
            if empty {
                initial["players"]["P2"]["zones"]["field"] = json!([]);
            }
            let mut game = game(&initial);
            enter_end(&mut game);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if pp >= 2 { 18_u8 } else { 20 },
                "zero follower candidates must not suppress the leader clause"
            );
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(pp >= 4),
                "the four PP threshold adds exactly one draw"
            );
            assert_eq!(
                state["P2"]["cemetery"],
                if !empty && pp >= 6 {
                    json!(["enemy"])
                } else {
                    json!([])
                },
                "only the six PP clause destroys the optional target"
            );
        }
    }
}
#[test]
fn evolved_karla_declares_targets_before_recovery_then_reads_recovered_pp_for_all_clauses() {
    for empty in [false, true] {
        let mut initial = evolution("BP06-091", "BP06-092");
        initial["players"]["P1"]["pp"]["current"] = json!(5_u8);
        if empty {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut game = game(&initial);
        evolve(&mut game);
        enter_end(&mut game);
        let step = pending(&mut game);
        if !empty {
            assert!(
                matches!(step, CardPlay::Awaiting(_)),
                "Q885 requires a formal follower choice before recovery"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                4_u8,
                "PP recovery waits for the formal target answer"
            );
        }
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"], 6_u8,
            "the end ability recovers two after paying for evolution"
        );
        assert_eq!(
            state["P2"]["leader"]["life"], 18_u8,
            "the recovered PP enable leader damage even without a follower target"
        );
        assert_eq!(
            state["P1"]["hand"],
            json!(["deck0"]),
            "the recovered PP also enable draw"
        );
        assert_eq!(
            state["P2"]["cemetery"],
            if empty { json!([]) } else { json!(["enemy"]) },
            "the six PP condition reads after recovery"
        );
    }
}
#[test]
fn hidden_blade_end_damage_requires_two_remaining_pp() {
    for pp in [1_u8, 2] {
        let mut initial = position("BP06-094", "field");
        initial["players"]["P1"]["pp"]["current"] = json!(pp);
        let mut game = game(&initial);
        enter_end(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if pp == 2 { 2_u8 } else { 0 },
            "the formal target does not bypass the remaining PP condition"
        );
    }
}
#[test]
fn evolved_hidden_blade_damages_on_evolution_and_again_at_its_two_pp_end() {
    let mut initial = evolution("BP06-094", "BP06-095");
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8,
        "evolution deals the first two damage"
    );
    enter_end(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8,
        "two PP remain for the end ability"
    );
}
