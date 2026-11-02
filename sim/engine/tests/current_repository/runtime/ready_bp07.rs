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
const CONTROLS: [SupportCard; 18] = [
    (
        "forest-base",
        "ブルームスピリット",
        "エルフ",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "witch-base",
        "天外の華・エレノア",
        "ウィッチ",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "dragon-base",
        "慈愛の竜戦士",
        "ドラゴン",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "nightmare-base",
        "禁絶の腕・ニコラ",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "bishop-base",
        "欠落の聖女・リモニウム",
        "ビショップ",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "neutral-base",
        "メカゴブリン",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    ("one", "ordinary", "ニュートラル", "フォロワー", "1", &[]),
    (
        "machine",
        "machine",
        "ニュートラル",
        "フォロワー",
        "1",
        &["機械"],
    ),
    (
        "machine2",
        "two cost machine",
        "ニュートラル",
        "フォロワー",
        "2",
        &["機械"],
    ),
    (
        "machine3",
        "expensive machine",
        "ニュートラル",
        "フォロワー",
        "3",
        &["機械"],
    ),
    (
        "machinespell",
        "machine spell",
        "ウィッチ",
        "スペル",
        "1",
        &["機械"],
    ),
    (
        "natural",
        "natural",
        "ニュートラル",
        "アミュレット",
        "1",
        &["自然"],
    ),
    (
        "dragon2",
        "small dragon",
        "ドラゴン",
        "フォロワー",
        "2",
        &[],
    ),
    (
        "dragon4",
        "large dragon",
        "ドラゴン",
        "フォロワー",
        "4",
        &[],
    ),
    ("artspell", "美麗なる術式", "ウィッチ", "スペル", "2", &[]),
    (
        "prior",
        "free prior spell",
        "ニュートラル",
        "スペル",
        "0",
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
    (
        "amulet",
        "ordinary amulet",
        "ニュートラル",
        "アミュレット",
        "1",
        &[],
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
        let ability = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for (number, ..) in CONTROLS {
            if number.ends_with("-base") { tree["cards"][number] = ability.clone(); }
        }
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        let mut action = template["abilities"][0].clone();
        action.as_object_mut().unwrap().remove("evolve");
        action["activate"] = json!({"cost":{"pp":0_u8},"do":[{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower","other":true}}}]});
        tree["cards"]["destroyer"] = json!({"meta":template["meta"],"abilities":[action]});
        let mut programs = lowerer().lower(&tree).unwrap();
        let mut spell_tree = expanded::spell_tree().clone();
        let spell_template = spell_tree["cards"]["SD03-005"].clone();
        spell_tree["cards"] = json!({});
        let mut costs = BTreeMap::new();
        for (number, cost) in [("prior", 0_u64), ("artspell", 2), ("machinespell", 1)] {
            let mut spell = spell_template.clone();
            spell["abilities"][0]["spell"] = json!({"do":[]});
            spell_tree["cards"][number] = spell;
            costs.insert(number.to_owned(), Nat(cost));
        }
        programs.extend(lowerer().lower_with_printed_costs(&spell_tree, &costs).unwrap());
        (selected, programs)
    })
}
fn game(initial: &Value) -> Game {
    let (selected, bases) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp07",
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
fn activate_card(game: &mut Game, number: &str, line: u32) {
    let ability = prepared().0.current[number]
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
fn latika_requires_five_real_plays_to_evolve_and_recovers_pp_only_for_a_named_tree() {
    for prior in [4_usize, 5] {
        let mut initial = evolution("BP07-001", "BP07-002");
        initial["players"]["P1"]["zones"]["hand"] = copies("prior", "prior", prior);
        let mut game = game(&initial);
        for n in 0..prior {
            play(&mut game, &format!("prior{n}"));
        }
        let before = game.projection(View::Referee).unwrap();
        let outcome = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(
            outcome.outcome,
            if prior == 5 {
                "resolved"
            } else {
                "cannot-evolve"
            },
            "the printed guard counts five committed plays"
        );
        if prior == 4 {
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "a rejected guard consumes neither PP nor evolution resource"
            );
        } else {
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["evolved_with"], "form",
                "the real matching form is consumed"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 9_u8,
                "the enabled evolution pays one PP"
            );
        }
    }
    for card in ["BP07-T03", "amulet"] {
        let mut initial = position("BP07-001", "field");
        initial["players"]["P1"]["pp"]["current"] = json!(8_u8);
        initial["players"]["P1"]["zones"][if card == "BP07-T03" { "ex" } else { "hand" }] =
            json!([{"id":"entry","card":card}]);
        let mut game = game(&initial);
        play(&mut game, "entry");
        if card == "BP07-T03" {
            trigger(&mut game);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if card == "BP07-T03" { 8_u8 } else { 7 },
            "only the named tree refunds its one PP play cost"
        );
    }
}
#[test]
fn evolved_bloom_spirit_reads_all_ex_cards_at_the_two_three_boundary() {
    for count in [2_usize, 3] {
        let mut initial = evolution("forest-base", "BP07-009");
        initial["players"]["P1"]["zones"]["ex"] = copies("one", "ex", count);
        let mut game = game(&initial);
        evolve(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if count == 3 { 4_u8 } else { 2 },
            "the third EX card replaces two damage with four"
        );
    }
}
#[test]
fn eleanor_follows_only_the_searched_spell_with_the_spellchain_discount() {
    for graves in [9_usize, 10] {
        let mut initial = evolution("witch-base", "BP07-040");
        initial["players"]["P1"]["zones"]["cemetery"] = copies("prior", "grave", graves);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"artspell"},{"id":"wrong","card":"one"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"other","card":"artspell"}]);
        let mut game = game(&initial);
        evolve(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["found"]),
            "the printed name is searched to EX"
        );
        play(&mut game, "found");
        let after = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_u64()
            .unwrap();
        assert_eq!(
            after,
            if graves == 10 { 9 } else { 7 },
            "only Spellchain ten makes the searched two cost spell free"
        );
        play(&mut game, "other");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            after - 2,
            "the receipt discount does not affect another copy"
        );
    }
}
#[test]
fn mechanical_sorcerer_offers_both_tokens_then_counts_ex_machines_after_payment() {
    for mode in [0_usize, 1] {
        let mut initial = position("BP07-046", "hand");
        initial["players"]["P1"]["zones"]["ex"] = copies("machine", "fee", 2);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger_with(&mut game, Decisions { mode, ..DEFAULT });
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            named(
                &before,
                "ex",
                if mode == 0 {
                    "プロダクトマシーン"
                } else {
                    "リペアモード"
                }
            ),
            1,
            "each printed resolution alternative creates its full token template"
        );
        activate_card(&mut game, "BP07-046", 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["banish"].as_array().unwrap().len(),
            1,
            "one EX machine pays the material cost"
        );
        assert_eq!(
            state["P1"]["ex"].as_array().unwrap().len(),
            2,
            "two machines remain after the fee"
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"], 2_u8,
            "Q1011 counts the two remaining machines rather than the original three"
        );
        assert_eq!(
            state["objects"]["source"]["acted"], true,
            "the source also acts as a cost"
        );
    }
}
#[test]
fn prototype_machine_can_choose_either_mode_and_needs_three_ex_machines_for_the_buff() {
    for count in [2_usize, 3] {
        for mode in [0_usize, 1] {
            let mut initial = position("BP07-047", "hand");
            initial["players"]["P1"]["zones"]["ex"] = copies("machine", "ex", count);
            let mut game = game(&initial);
            play(&mut game, "source");
            trigger_with(&mut game, Decisions { mode, ..DEFAULT });
            let state = game.projection(View::Referee).unwrap();
            let enabled = mode == 1 && count == 3;
            assert_eq!(
                state["objects"]["source"]["power"],
                if enabled { 3_u8 } else { 2 },
                "only the enabled second mode adds power"
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if enabled { 2_u8 } else { 1 },
                "the same mode adds life"
            );
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進")),
                enabled,
                "Rush is granted only by the enabled second mode"
            );
            assert_eq!(
                named(&state, "ex", "プロダクトマシーン"),
                usize::from(mode == 0),
                "only the first mode creates a Product Machine"
            );
        }
    }
}
#[test]
fn wisdom_core_can_decline_its_look_pick_and_retire_with_three_ex_machines() {
    for take in [0_usize, 1] {
        let mut initial = position("BP07-051", "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"pick","card":"machine"},{"id":"wrong","card":"one"},{"id":"tail","card":"one"}]);
        initial["players"]["P1"]["zones"]["ex"] = copies("machine", "ex", 3);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger_with(&mut game, Decisions { take, ..DEFAULT });
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["hand"],
            if take == 1 {
                json!(["pick"])
            } else {
                json!([])
            },
            "the machine pick is optional"
        );
        assert_eq!(
            before["P1"]["deck"],
            if take == 1 {
                json!(["tail", "wrong"])
            } else {
                json!(["tail", "wrong", "pick"])
            },
            "unchosen observations return to the bottom in the chosen order"
        );
        activate_card(&mut game, "BP07-051", 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["source"]),
            "the core itself pays the retirement cost"
        );
        assert_eq!(
            state["P2"]["leader"]["life"], 19_u8,
            "the eligible activation damages the opposing leader"
        );
    }
}
#[test]
fn marion_looks_only_when_awakened_and_takes_a_dragon_follower_to_ex() {
    for maximum in [6_u8, 7] {
        let mut initial = position("BP07-056", "hand");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"small","card":"dragon2"},{"id":"wrong","card":"one"},{"id":"large","card":"dragon4"},{"id":"tail","card":"one"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["ex"],
            if maximum == 7 {
                json!(["small"])
            } else {
                json!([])
            },
            "Awakened permits a Dragon follower among the observed three"
        );
        assert_eq!(
            state["P1"]["deck"],
            if maximum == 7 {
                json!(["tail", "large", "wrong"])
            } else {
                json!(["small", "wrong", "large", "tail"])
            },
            "the rest return to bottom only after an awakened look"
        );
    }
}
#[test]
fn evolved_dragon_warrior_returns_only_a_small_dragon_follower() {
    let mut initial = evolution("dragon-base", "BP07-066");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"costly","card":"dragon4"},{"id":"wrong","card":"one"},{"id":"small","card":"dragon2"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "small"]),
        "only the original cost at most three Dragon follower returns"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["costly", "wrong"]),
        "wrong cost and class remain in the cemetery"
    );
}
#[test]
fn mono_requires_five_machine_followers_to_evolve_and_can_create_only_once_per_turn() {
    for others in [3_usize, 4] {
        let mut initial = evolution("BP07-069", "BP07-070");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend(
                copies("machine", "ally", others)
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned(),
            );
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(
            step.outcome,
            if others == 4 {
                "resolved"
            } else {
                "cannot-evolve"
            },
            "Mono itself is the fifth machine follower"
        );
        if others == 3 {
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "a four follower field cannot pay the guarded evolution"
            );
        }
    }
    let mut initial = position("BP07-069", "field");
    initial["players"]["P1"]["zones"]["cemetery"] = copies("machine", "fee", 4);
    let mut game = game(&initial);
    activate_card(&mut game, "BP07-069", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["banish"].as_array().unwrap().len(),
        2,
        "exactly two graveyard machines are paid"
    );
    assert_eq!(
        named(&state, "field", "プロダクトマシーン"),
        1,
        "the activation creates one complete Product Machine"
    );
    let ability = prepared().0.current["BP07-069"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == 2)
        .unwrap()
        .node
        .id;
    game.activate_internal_card("source", ability, PLAYER)
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        state,
        "the once per turn quota rejects a second use despite remaining materials"
    );
}
#[test]
fn first_one_has_mode_specific_formal_targets_and_cannot_play_without_any_eligible_follower() {
    for mode in [0_usize, 1] {
        let mut initial = position("BP07-075", "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrongspell","card":"machinespell"},{"id":"ally","card":"machine"},{"id":"mono","card":"BP07-069"},{"id":"wrongtrait","card":"one"}]);
        let mut game = game(&initial);
        play_with(&mut game, Decisions { mode, ..DEFAULT });
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if mode == 0 {
                json!(["ally"])
            } else {
                json!([])
            },
            "the first mode returns a selected machine follower to hand"
        );
        assert_eq!(
            state["P1"]["field"],
            if mode == 1 {
                json!(["mono"])
            } else {
                json!([])
            },
            "only the second mode puts the named Mono onto the field"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("wrongspell")),
            "a machine spell is not a mode one follower target"
        );
    }
    let mut mono_initial = position("BP07-075", "hand");
    mono_initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"mono","card":"BP07-069"}]);
    let mut mono = game(&mono_initial);
    play_with(&mut mono, DEFAULT);
    let state = mono.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["mono"]),
        "Q1027 choosing Mono with mode one puts it in hand rather than on the field"
    );
    assert_eq!(
        state["P1"]["field"],
        json!([]),
        "mode one does not revive the named follower"
    );
    let mut empty_initial = position("BP07-075", "hand");
    empty_initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"wrong","card":"machinespell"}]);
    let mut empty = game(&empty_initial);
    let before = empty.projection(View::Referee).unwrap();
    assert!(
        matches!(
            empty.play_internal_card("source", PLAYER),
            Err(EngineFailure::IllegalPlay(_)) | Ok(CardPlay::Cancelled)
        ),
        "Q1026 rejects a spell with no legal target for either mode"
    );
    assert_eq!(
        empty.projection(View::Referee).unwrap(),
        before,
        "a targetless attempt consumes neither the spell nor PP"
    );
}
#[test]
fn nicola_moves_any_small_machine_kind_to_ex_and_discounts_only_that_object() {
    for (card, original) in [("machine2", 2_u64), ("machinespell", 1)] {
        let mut initial = evolution("nightmare-base", "BP07-077");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrongtrait","card":"one"},{"id":"costly","card":"machine3"},{"id":"picked","card":card}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"other","card":card}]);
        let mut game = game(&initial);
        evolve(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["picked"]),
            "both follower and spell machine cards of cost at most two are eligible"
        );
        play(&mut game, "picked");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8,
            "the receipt's minus two makes the EX object free"
        );
        play(&mut game, "other");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9 - original,
            "an unrelated copy pays its original cost"
        );
    }
}
#[test]
fn bone_drone_lastword_creates_a_complete_product_machine_on_field() {
    let mut initial = position("BP07-081", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"destroyer","card":"destroyer"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    support(&mut game, "destroyer", "destroyer");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&state, "field", "プロダクトマシーン"),
        1,
        "the lastword uses the admitted Product Machine template"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["source"]),
        "the destroyed Drone is in its cemetery"
    );
}
#[test]
fn blood_core_looks_for_a_machine_but_cannot_count_its_own_retirement_toward_the_guard() {
    for graves in [4_usize, 5] {
        let mut initial = position("BP07-085", "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = copies("machine", "grave", graves);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"machinespell"},{"id":"wrong","card":"one"},{"id":"tail","card":"one"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["hand"],
            json!(["picked"]),
            "the look accepts a machine spell and rejects the ordinary follower"
        );
        if graves == 4 {
            let ability = prepared().0.current["BP07-085"]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| a.source.line == 2)
                .unwrap()
                .node
                .id;
            assert!(
                matches!(
                    game.activate_internal_card("source", ability, PLAYER),
                    Err(EngineFailure::IllegalPlay(_))
                ),
                "four graveyard machines cannot enable an activation that would retire the fifth"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "the failed guard preserves the core and all resources"
            );
        } else {
            activate_card(&mut game, "BP07-085", 2);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                named(&state, "ex", "プロダクトマシーン"),
                1,
                "five prior graveyard machines enable one token in EX"
            );
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                "the core pays its own retirement cost"
            );
        }
    }
}
#[test]
fn limonium_destroys_only_enemy_followers_then_revives_a_small_machine_once() {
    let mut initial = evolution("bishop-base", "BP07-087");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"one"},{"id":"second","card":"one"},{"id":"amulet","card":"amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"spell","card":"machinespell"},{"id":"wrong","card":"one"},{"id":"costly","card":"machine3"},{"id":"small","card":"machine2"},{"id":"spare","card":"machine2"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!(["amulet"]),
        "the evolution destroys every opposing follower but no amulet"
    );
    activate_card(&mut game, "BP07-087", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "small"]),
        "only a machine follower of original cost at most two returns"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 9_u8,
        "the printed activation costs zero PP after the one PP evolution"
    );
    let ability = prepared().0.current["BP07-087"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == 2)
        .unwrap()
        .node
        .id;
    game.activate_internal_card("source", ability, PLAYER)
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        state,
        "once per turn prevents reviving the remaining eligible machine"
    );
}
#[test]
fn machine_angel_has_ward_offers_both_tokens_and_requires_five_graveyard_machines() {
    for mode in [0_usize, 1] {
        for graves in [4_usize, 5] {
            let mut initial = position("BP07-106", "hand");
            initial["players"]["P1"]["zones"]["cemetery"] = copies("machine", "grave", graves);
            let mut game = game(&initial);
            play(&mut game, "source");
            trigger_with(&mut game, Decisions { mode, ..DEFAULT });
            let before = game.projection(View::Referee).unwrap();
            assert!(
                before["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護")),
                "the printed Ward is retained"
            );
            assert_eq!(
                named(
                    &before,
                    "ex",
                    if mode == 0 {
                        "プロダクトマシーン"
                    } else {
                        "リペアモード"
                    }
                ),
                1,
                "both printed token alternatives use admitted templates"
            );
            if graves == 4 {
                let ability = prepared().0.current["BP07-106"]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|a| a.source.line == 3)
                    .unwrap()
                    .node
                    .id;
                assert!(
                    matches!(
                        game.activate_internal_card("source", ability, PLAYER),
                        Err(EngineFailure::IllegalPlay(_))
                    ),
                    "the four machine graveyard does not meet the guard"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "a rejected activation does not act the source"
                );
            } else {
                activate_card(&mut game, "BP07-106", 3);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["objects"]["enemy"]["damage"], 4_u8,
                    "the eligible activation deals four to its formal target"
                );
                assert_eq!(
                    state["objects"]["source"]["acted"], true,
                    "acting the source pays the cost"
                );
            }
        }
    }
}
