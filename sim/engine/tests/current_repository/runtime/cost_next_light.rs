#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate migration scenarios."
)]
#![expect(
    clippy::default_numeric_fallback,
    reason = "Synthetic costs and PP expectations use signed integers."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

struct Fixture {
    selection: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = Vec::new();
        for (id, name, cost, traits, spell, evolve) in [
            ("light-yokai", "light-yokai", 3, vec!["妖怪"], false, false),
            ("light-enjoy", "light-enjoy", 3, vec!["宴楽"], false, false),
            ("light-wizard", "light-wizard", 3, vec!["魔法使い"], false, false),
            ("light-prison", "light-prison", 3, vec!["八獄"], false, false),
            ("light-hunter", "light-hunter", 3, vec!["絶傑", "狩人"], false, false),
            ("light-treasure", "light-treasure", 3, vec!["財宝"], false, false),
            ("light-nightmare", "light-nightmare", 3, vec!["NIGHTMARE"], false, false),
            ("light-passion", "light-passion", 3, vec!["パッション"], true, false),
            ("light-spell", "light-spell", 1, vec![], true, false),
            ("light-ann", "アンの大魔法", 5, vec!["学院"], true, false),
            ("light-soo", "独尊龍・スーロン", 1, vec![], false, true),
            ("light-octo", "簒奪の団結者", 1, vec![], false, true),
            ("light-ann-base", "精彩の魔導・アン", 1, vec![], false, true),
            ("light-jun", "ジュン", 1, vec![], false, true),
            ("light-brawler", "鉄拳の獣戦士", 1, vec![], false, true),
        ] {
            let face = json!({"name":name,"card_type":if spell {"スペル"} else {"フォロワー"},"card_class":"ニュートラル","cost":cost.to_string(),"power":if spell {"-"} else {"1"},"hp":if spell {"-"} else {"1"},"traits":traits,"text":null,"sections":[]});
            snapshot.push_str(&json!({"number":id,"faces":[face]}).to_string()); snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{id}.yaml"),json!({"format":1,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}}).to_string()]));
            let program = if evolve {
                let mut tree = expanded::tree().clone();
                tree["cards"] = json!({id:tree["cards"]["synthetic-evolution"]});
                lowerer().lower(&tree).unwrap().remove(id).unwrap()
            } else if spell {
                let mut tree = expanded::spell_tree().clone();
                let mut candidate = tree["cards"]["SD03-005"].clone();
                candidate["abilities"][0]["spell"] = json!({"do":[]});
                tree["cards"] = json!({id:candidate});
                lowerer().lower_with_printed_costs(&tree, &BTreeMap::from([(id.to_owned(), Nat(u64::try_from(cost).unwrap()))])).unwrap().remove(id).unwrap()
            } else { WellFormedCardProgram::check(CardProgram{abilities:vec![],card_play_costs:vec![]}).unwrap() };
            programs.push((id.to_owned(),program));
        }
        Fixture { selection:repository::selection_with_rules_from(&snapshot,&keywords,&input), programs }
    })
}
fn game(initial: &Value) -> Game {
    let fixture = fixture();
    let mut game = Game::from_selected_effects(
        fixture.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cost-next-light",
    )
    .unwrap();
    for (id, program) in &fixture.programs {
        game.load_internal_card(id, program).unwrap();
    }
    game
}
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "The scenarios reject every unhandled question."
)]
fn finish(game: &mut Game, mut result: CardPlay) {
    while let CardPlay::Awaiting(question) = result {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|member| member.handle)
                    .collect(),
            )),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            InputDomain::Modes { modes, count, .. } => Answer::Modes(
                modes
                    .iter()
                    .copied()
                    .take(usize::try_from(count.0).unwrap())
                    .collect(),
            ),
            _ => panic!("unexpected migration question"),
        };
        result = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    drain(game);
}
fn drain(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending")
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
fn play(game: &mut Game, id: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, result);
}
fn pp(game: &Game) -> i64 {
    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_i64()
        .unwrap()
}
fn position(source: &str, candidate: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":source},{"id":"candidate","card":candidate}]);
    initial
}
fn evolve(game: &mut Game, super_evolve: bool) {
    game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":if super_evolve {json!({"pp":1,"sep":1})} else {json!({"pp":1})}})).unwrap();
    drain(game);
}
fn form_position(base: &str, form: &str, candidate: &str) -> Value {
    let mut initial = position(base, candidate);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"candidate","card":candidate}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
#[test]
fn ginsetsu_reduces_a_later_yokai_follower_by_two() {
    let mut initial = position("BP06-074", "light-yokai");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"candidate","card":"light-yokai"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP06-074"}]);
    let mut game = game(&initial);
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 9);
}
#[test]
fn avatar_reduces_the_tree_it_creates_in_ex() {
    let mut game = game(&position("BP07-010", "BP01-171"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let tree = state["P1"]["ex"][0].as_str().unwrap().to_owned();
    play(&mut game, &tree);
    assert_eq!(pp(&game), 8);
}
#[test]
fn hozumi_reduces_the_next_festive_card() {
    let mut game = game(&position("BP14-001", "light-enjoy"));
    play(&mut game, "source");
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 6);
}
#[test]
fn owl_accepts_either_printed_trait_for_the_next_discount() {
    for candidate in ["light-enjoy", "light-wizard"] {
        let mut game = game(&position("BP14-049", candidate));
        play(&mut game, "source");
        play(&mut game, "candidate");
        assert_eq!(pp(&game), 7);
    }
}
#[test]
fn soolong_installs_the_next_discount_on_actual_evolution() {
    let mut game = game(&form_position("light-soo", "BP14-054", "light-enjoy"));
    evolve(&mut game, false);
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 8);
}
#[test]
fn miroro_discounts_the_small_spell_and_its_play_buffs_a_follower() {
    let mut game = game(&position("BP18-016", "light-spell"));
    play(&mut game, "source");
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 7);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}
#[test]
fn assassin_installs_the_next_prison_card_discount() {
    let mut game = game(&position("BP19-011", "light-prison"));
    play(&mut game, "source");
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 6);
}
#[test]
fn eldias_zero_pp_activation_discounts_both_required_traits_and_uses_its_quota() {
    let mut initial = position("BP20-001", "light-hunter");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"candidate","card":"light-hunter"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-001"}]);
    let mut game = game(&initial);
    let id = fixture().selection.current["BP20-001"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let result = game
        .activate_internal_card("source", id, PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, result);
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 9);
    game.activate_internal_card("source", id, PlayerId { seat: 1 })
        .unwrap_err();
}
#[test]
fn octrice_evolution_damages_then_discounts_the_next_treasure() {
    let mut initial = form_position("light-octo", "BP20-024", "light-treasure");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game, false);
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 8);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn ann_super_evolution_searches_then_discounts_the_named_spell() {
    let mut initial = form_position("light-ann-base", "BP21-039", "BP01-171");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"spell","card":"light-ann"}]);
    initial["players"]["P1"]["sep"] = json!(1);
    initial["turn"]["elapsed_turns"]["P1"] = json!(7);
    let mut game = game(&initial);
    evolve(&mut game, true);
    play(&mut game, "spell");
    assert_eq!(pp(&game), 9);
}
#[test]
fn formation_fills_ex_then_discounts_only_a_royal_token() {
    let mut initial = position("BP22-030", "BP01-171");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"old1","card":"BP01-171"},{"id":"old2","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    let token = state["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| !id.starts_with("old"))
        .unwrap()
        .to_owned();
    play(&mut game, &token);
    assert_eq!(pp(&game), 9);
}
#[test]
fn jun_union_burst_searches_then_discounts_the_nightmare_follower() {
    let mut initial = form_position("light-jun", "CP04-024", "BP01-171");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"nightmare","card":"light-nightmare"}]);
    let mut game = game(&initial);
    evolve(&mut game, false);
    play(&mut game, "nightmare");
    assert_eq!(pp(&game), 8);
}
#[test]
fn tomoe_discounts_a_small_passion_spell_then_triggers_two_damage() {
    let mut game = game(&position("ECP02-036", "light-passion"));
    play(&mut game, "source");
    play(&mut game, "candidate");
    assert_eq!(pp(&game), 7);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
}
#[test]
fn ironfist_evolution_moves_the_top_card_to_ex_then_discounts_its_play() {
    let mut initial = form_position("light-brawler", "BP12-022", "BP01-171");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"light-yokai"}]);
    let mut game = game(&initial);
    evolve(&mut game, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["drawn"])
    );
    play(&mut game, "drawn");
    assert_eq!(pp(&game), 9);
}
#[test]
fn warden_damages_looks_reveals_and_discounts_the_selected_small_wizard() {
    let mut initial = position("BP19-045", "BP01-171");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wizard","card":"light-wizard"},{"id":"rest1","card":"BP01-171"},{"id":"rest2","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2);
    assert_eq!(state["P1"]["cemetery"], json!(["rest1", "rest2"]));
    play(&mut game, "wizard");
    assert_eq!(pp(&game), 5);
}
