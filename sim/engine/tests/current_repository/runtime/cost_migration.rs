#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate migrated card scenarios."
)]
use super::*;
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial
}
struct Fixture {
    selection: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = Vec::new();
        // Explicit synthetic companions isolate name conditions and the evolution procedure.
        // They do not admit or replace the corresponding printed cards.
        for (id, name, evolve) in [
            ("cost-gilnelise", "飢餓の絶傑・ギルネリーゼ", false),
            ("cost-nacht", "カースドクイーン・ナハト・ナハト", false),
            ("cost-hozumi", "化かし女将・ホズミ", false),
            ("cost-caryl-base", "キャル", true),
        ] {
            let face = json!({"name":name,"card_type":"フォロワー","card_class":"ニュートラル","cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]});
            snapshot.push_str(&json!({"number":id,"faces":[face]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{id}.yaml"),json!({"format":1_i64,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}}).to_string()]));
            let program = if evolve {
                let mut tree = expanded::tree().clone();
                tree["cards"] = json!({id:tree["cards"]["synthetic-evolution"].clone()});
                lowerer().lower(&tree).unwrap().remove(id).unwrap()
            } else {WellFormedCardProgram::check(CardProgram{abilities:vec![],card_play_costs:vec![]}).unwrap()};
            programs.push((id.into(),program));
        }
        Fixture{selection:repository::selection_with_rules_from(&snapshot,&keywords,&input),programs}
    })
}
fn game(initial: &Value) -> Game {
    let prepared = fixture();
    let mut game = Game::from_selected_effects(
        prepared.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cost-card-migration",
    )
    .unwrap();
    for (id, program) in &prepared.programs {
        game.load_internal_card(id, program).unwrap();
    }
    game
}
fn play(game: &mut Game, id: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, result);
    triggers(game);
}
fn finish(game: &mut Game, mut result: CardPlay) {
    while let CardPlay::Awaiting(question) = result {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            let mut members = domain.members.iter().collect::<Vec<_>>();
            members.sort_by_key(|member| {
                !matches!(
                    member.card.as_deref(),
                    Some("BP01-037" | "BP01-065" | "BP19-073")
                )
            });
            let maximum = domain
                .cardinality
                .maximum
                .unwrap_or(domain.cardinality.minimum)
                .0;
            Answer::Targets(
                members
                    .into_iter()
                    .take(usize::try_from(maximum).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if let InputDomain::TokenCapacity { choices } = &question.domain {
            assert_eq!(choices.len(), 1_usize, "finish");
            Answer::TokenCapacity(0)
        } else {
            panic!("unexpected migration input");
        };
        result = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
#[test]
fn bp18_008_counts_used_tokyo_forms_and_damages_the_targets_leader_after_destruction() {
    for count in [0_u8, 2] {
        let mut initial = position("BP18-008");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("used{i}"),"card":"BP18-072","face_up":true}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            6_u8.checked_add(count).unwrap(),
            "bp18 008 counts used tokyo forms and damages the targets leader after destruction"
        );
        assert_eq!(
            state["P2"]["cemetery"],
            json!(["enemy"]),
            "bp18 008 counts used tokyo forms and damages the targets leader after destruction"
        );
        assert_eq!(
            state["P2"]["leader"]["life"], 19_i64,
            "bp18 008 counts used tokyo forms and damages the targets leader after destruction"
        );
    }
}
#[test]
fn pr_148_counts_both_fields_then_deals_seven_to_all_followers() {
    let mut initial = position("PR-148");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 6_i64,
        "pr 148 counts both fields then deals seven to all followers"
    );
    assert_eq!(
        state["P1"]["field"],
        json!([]),
        "pr 148 counts both fields then deals seven to all followers"
    );
    assert_eq!(
        state["P2"]["field"],
        json!([]),
        "pr 148 counts both fields then deals seven to all followers"
    );
}

fn triggers(game: &mut Game) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed cost trigger");
        };
        let result = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, result);
    }
}
#[test]
fn bp05_106_needs_both_ex_and_named_follower_and_keeps_other_follower_rush() {
    for zone in ["hand", "ex"] {
        for named in [false, true] {
            let mut initial = position("BP05-106");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":"BP05-106"}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
            if named {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"name-carrier","card":"cost-gilnelise"}));
            }
            let mut game = game(&initial);
            play(&mut game, "source");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if named && zone == "ex" { 10_i64 } else { 7_i64 },
                "bp05 106 needs both ex and named follower and keeps other follower rush"
            );
            assert!(
                state["objects"]["ally"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進")),
                "bp05 106 needs both ex and named follower and keeps other follower rush"
            );
        }
    }
}
#[test]
fn bp05_113_ex_and_named_follower_condition_preserves_four_damage_and_one_heal() {
    for named in [false, true] {
        let mut initial = position("BP05-113");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP05-113"}]);
        if named {
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"name-carrier","card":"cost-gilnelise"}]);
        }
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if named { 10_i64 } else { 7_i64 },
            "bp05 113 ex and named follower condition preserves four damage and one heal"
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"], 4_i64,
            "bp05 113 ex and named follower condition preserves four damage and one heal"
        );
        assert_eq!(
            state["P1"]["leader"]["life"], 21_i64,
            "bp05 113 ex and named follower condition preserves four damage and one heal"
        );
    }
}
#[test]
fn bp11_031_ex_discount_and_draw_both_require_the_printed_name() {
    for named in [false, true] {
        let mut initial = position("BP11-031");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP11-031"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
        if named {
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"name-carrier","card":"cost-nacht"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if named { 10_i64 } else { 9_i64 },
            "bp11 031 ex discount and draw both require the printed name"
        );
        assert_eq!(
            state["P1"]["hand"],
            if named { json!(["drawn"]) } else { json!([]) },
            "bp11 031 ex discount and draw both require the printed name"
        );
    }
}
#[test]
fn bp14_007_combines_independent_ex_and_named_field_discounts_then_bounces_and_heals() {
    for zone in ["hand", "ex"] {
        for named in [false, true] {
            let mut initial = position("BP14-007");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":"BP14-007"}]);
            if named {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"name-carrier","card":"cost-hozumi"}]);
            }
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            let mut game = game(&initial);
            play(&mut game, "source");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                8_u8.checked_add(u8::from(zone == "ex"))
                    .unwrap()
                    .checked_add(u8::from(named))
                    .unwrap(),
                "bp14 007 combines independent ex and named field discounts then bounces and heals"
            );
            assert_eq!(
                state["P2"]["hand"],
                json!(["enemy"]),
                "bp14 007 combines independent ex and named field discounts then bounces and heals"
            );
            assert_eq!(
                state["P1"]["leader"]["life"], 21_i64,
                "bp14 007 combines independent ex and named field discounts then bounces and heals"
            );
        }
    }
}
#[test]
fn cp04_038_real_super_evolution_returns_distinct_spells_and_discounts_their_actual_ex_plays() {
    let mut initial = position("CP04-038");
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_i64);
    initial["players"]["P1"]["sep"] = json!(1_i64);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"cost-caryl-base"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-038"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"spell-one","card":"BP01-065"},{"id":"spell-two","card":"BP19-073"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..3)
            .map(|i| json!({"id":format!("drawn{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["cemetery"]
        .as_array_mut()
        .unwrap()
        .extend((0_u8..8).map(|i| json!({"id":format!("unchosen{i}"),"card":"BP01-071"})));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_i64,"sep":1_i64}})
        )
        .unwrap()
        .outcome,
        "resolved", "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays");
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["field"],
        json!([]),
        "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays"
    );
    assert_eq!(
        state["P1"]["ex"],
        json!(["spell-one", "spell-two"]),
        "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays"
    );
    assert_eq!(
        state["P1"]["cemetery"].as_array().unwrap().len(),
        8_usize,
        "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays"
    );
    play(&mut game, "spell-one");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        8_i64,
        "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays"
    );
    play(&mut game, "spell-two");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        8_i64,
        "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn0", "drawn1", "drawn2"]),
        "cp04 038 real super evolution returns distinct spells and discounts their actual ex plays"
    );
}
