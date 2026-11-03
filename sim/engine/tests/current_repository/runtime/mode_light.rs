#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed payment scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

/// Synthetic materials isolate the admitted programs from unrelated card abilities.
const STAND_INS: [(&str, &str, &str, &[&str], u8); 10] = [
    (
        "pay-unit",
        "test unit",
        "フォロワー",
        &["ウマ娘", "自然", "アイドル"],
        1,
    ),
    (
        "pay-royal",
        "pay-royal",
        "フォロワー",
        &["ロイヤルパラディン"],
        1,
    ),
    (
        "pay-shadow",
        "pay-shadow",
        "フォロワー",
        &["シャドウパラディン"],
        1,
    ),
    ("pay-pale", "pay-pale", "フォロワー", &["ペイルムーン"], 1),
    ("pay-kagero", "pay-kagero", "フォロワー", &["かげろう"], 1),
    ("pay-aqua", "pay-aqua", "フォロワー", &["アクアフォース"], 1),
    (
        "pay-oracle",
        "pay-oracle",
        "フォロワー",
        &["オラクルシンクタンク"],
        1,
    ),
    ("pay-amulet", "test amulet", "アミュレット", &[], 5),
    ("pay-lara-a", "ララミアA", "フォロワー", &[], 1),
    ("pay-lara-b", "ララミアB", "フォロワー", &[], 1),
];

fn prepared() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        for (id, name, kind, traits, cost) in STAND_INS {
            snapshot.push_str(&json!({"number":id,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":traits,"cost":cost.to_string(),"power":if kind == "フォロワー" {"1"} else {"-"},"hp":if kind == "フォロワー" {"10"} else {"-"},"text":"","sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{id}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn printings(sample: &str) -> Vec<String> {
    let id = &prepared().current[sample].card_id;
    prepared()
        .current
        .iter()
        .filter(|(_, c)| &c.card_id == id)
        .map(|(number, _)| number.clone())
        .collect()
}
fn load_stand_ins(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in STAND_INS {
        game.load_internal_card(number, &empty).unwrap();
    }
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "payment-light",
    )
    .unwrap();
    load_stand_ins(&mut game);
    game
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":card},{"id":"fee","card":"pay-unit"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top1","card":"pay-unit"},{"id":"top2","card":"pay-unit"},{"id":"tail","card":"pay-unit"},{"id":"last","card":"pay-unit"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"pay-unit"}]);
    initial
}
fn answer(q: &DecisionView, mode: u64, accept: bool) -> Answer {
    match &q.domain {
        InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
        InputDomain::OptionalCost => Answer::OptionalCost(accept),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::Targets(domain) => Answer::Targets(
            domain
                .members
                .iter()
                .filter(|member| member.card.is_some())
                .chain(domain.members.iter().filter(|member| member.card.is_none()))
                .take(usize::try_from(domain.cardinality.minimum.0.max(1)).unwrap())
                .map(|member| member.handle)
                .collect(),
        ),
        InputDomain::CostMaterials(domain) => Answer::CostMaterials(
            domain
                .members
                .iter()
                .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
            domain.cards.iter().map(|c| c.handle).collect(),
        )),
        InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
        _ => panic!("unexpected light payment question"),
    }
}
/// Card-specific assertions need to inspect a live decision before answering it.
fn until(
    game: &mut Game,
    mut play: CardPlay,
    mode: u64,
    accept: bool,
    stop: fn(&InputDomain) -> bool,
) -> (CardPlay, DecisionView) {
    loop {
        let CardPlay::Awaiting(q) = &play else {
            panic!("the inspected question is reached")
        };
        if stop(&q.domain) {
            let q = q.clone();
            return (play, q);
        }
        play = game
            .answer_internal_card(q.actor, q.token, answer(q, mode, accept))
            .unwrap();
    }
}
fn settle(game: &mut Game, mut play: CardPlay, mode: u64, accept: bool) {
    loop {
        while let CardPlay::Awaiting(q) = play {
            play = game
                .answer_internal_card(q.actor, q.token, answer(&q, mode, accept))
                .unwrap();
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending trigger")
        };
        play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
    }
}
fn play(game: &mut Game, mode: u64, accept: bool) {
    let result = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    settle(game, result, mode, accept);
}
fn quick_case(sample: &str, material: &str, check: fn(&Value)) {
    for number in printings(sample) {
        for accept in [true, false] {
            let mut initial = position(&number);
            initial["players"]["P1"]["zones"]["hand"][1]["card"] = json!(material);
            initial["players"]["P1"]["zones"]["deck"][0]["card"] = json!(material);
            let mut game = game(&initial);
            play(&mut game, 2, accept);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if accept { 3_i64 } else { 0_i64 },
                "{number}: refusing suppresses the paid damage"
            );
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fee")),
                accept,
                "{number}: only acceptance discards the series card"
            );
            if accept {
                check(&state);
            } else {
                assert_eq!(
                    state["P2"]["leader"]["life"], 20_i64,
                    "{number}: refusing applies none of the paid body"
                );
            }
        }
    }
}
#[test]
fn csd03a_007_pays_royal_paladin_and_heals() {
    quick_case("CSD03a-007", "pay-royal", |s| {
        assert_eq!(s["P1"]["leader"]["life"], 22_i64, "the paid mode heals two");
    });
}
#[test]
fn cp03_096_pays_shadow_paladin_and_mills_one() {
    quick_case("CP03-096", "pay-shadow", |s| {
        assert!(
            s["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("top1")),
            "the paid mode mills the top card"
        );
    });
}
#[test]
fn cp03_055_pays_pale_moon_and_banishes_three() {
    quick_case("CP03-055", "pay-pale", |s| {
        assert_eq!(
            s["P1"]["banish"],
            json!(["top1", "top2", "tail"]),
            "the paid mode banishes the top three"
        );
    });
}
#[test]
fn csd03b_007_pays_kagero_and_damages_the_targets_controller_leader() {
    quick_case("CSD03b-007", "pay-kagero", |s| {
        assert_eq!(
            s["P2"]["leader"]["life"], 19_i64,
            "the chosen follower's controller leader takes one damage"
        );
    });
}
#[test]
fn cp03_014_pays_aqua_force_and_takes_the_revealed_one_cost_follower() {
    quick_case("CP03-014", "pay-aqua", |s| {
        assert_eq!(
            s["P1"]["hand"],
            json!(["top1"]),
            "the looked-at Aqua Force follower is revealed and taken"
        );
    });
}
#[test]
fn cp03_117_pays_oracle_think_tank_and_splits_the_top_two() {
    quick_case("CP03-117", "pay-oracle", |s| {
        assert_eq!(
            s["P1"]["deck"],
            json!(["top1", "tail", "last", "top2"]),
            "the chosen card returns to top and the other goes under the remaining deck"
        );
    });
}
#[test]
fn bp05_041_paid_damage_uses_the_current_idol_count() {
    for number in printings("BP05-041") {
        let mut initial = position(&number);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"idol1","card":"pay-unit"},{"id":"idol2","card":"pay-unit"}]);
        let mut game = game(&initial);
        play(&mut game, 2, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_i64,
            "two field Idol cards determine the paid mode's damage"
        );
    }
}
#[test]
fn bp15_027_paid_search_puts_two_different_names_on_field() {
    for number in printings("BP15-027") {
        for accept in [true, false] {
            let mut initial = position(&number);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"lara1","card":"pay-lara-a"},{"id":"twin","card":"pay-lara-a"},{"id":"lara2","card":"pay-lara-b"}]);
            let mut game = game(&initial);
            let mut step = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            if accept {
                let (_, q) = until(&mut game, step, 2, true, |d| {
                    matches!(d, InputDomain::Search(_))
                });
                let InputDomain::Search(domain) = &q.domain else {
                    panic!("the requested decision domain is reached")
                };
                let named = |name: &str| {
                    domain
                        .cards
                        .iter()
                        .filter(|c| c.name == name)
                        .map(|c| c.handle)
                        .collect::<Vec<_>>()
                };
                // Grouped answers must exercise the name constraint rather than fail on shape.
                assert_eq!(
                    domain.groups.as_ref().map(Vec::len),
                    Some(1),
                    "distinct names use the printed group"
                );
                let before = format!("{game:?}");
                let rejected = game
                    .answer_internal_card(
                        q.actor,
                        q.token,
                        Answer::Search(SearchAnswer::Grouped(vec![named("ララミアA")])),
                    )
                    .unwrap_err();
                assert!(
                    format!("{rejected:?}").contains("search names must differ")
                        && format!("{game:?}") == before,
                    "{number}: a same-name pair is rejected and the search stays open"
                );
                let distinct = vec![named("ララミアA")[0], named("ララミアB")[0]];
                step = game
                    .answer_internal_card(
                        q.actor,
                        q.token,
                        Answer::Search(SearchAnswer::Grouped(vec![distinct])),
                    )
                    .unwrap();
            }
            settle(&mut game, step, 2, accept);
            let state = game.projection(View::Referee).unwrap();
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(
                (field.len(), field.contains(&json!("lara2"))),
                if accept { (2, true) } else { (0, false) },
                "{number}: only accepting four PP puts two distinct names on field"
            );
        }
    }
}
#[test]
fn bp12_106_paid_mode_targets_a_card_beyond_the_free_cost_limit() {
    for number in printings("BP12-106") {
        let mut initial = position(&number);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"pay-amulet"}]);
        let mut game = game(&initial);
        play(&mut game, 2, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["field"],
            json!([]),
            "the paid six-cost limit includes an opposing amulet"
        );
    }
}
#[test]
fn ecp01_029_evolve_mode_discards_three_for_two_max_pp() {
    for number in printings("ECP01-029") {
        let mut initial = position("ECP01-028");
        initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":5_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP01-028"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..3_u8)
                .map(|n| json!({"id":format!("fee-{n}"),"card":"pay-unit"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        let mut game = game(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
        )
        .unwrap();
        settle(&mut game, CardPlay::Cancelled, 2, true);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["pp"]["max"], 7_i64,
            "the three Uma Musume cards buy two maximum PP"
        );
        assert_eq!(
            s["P1"]["hand"],
            json!([]),
            "all three selected hand cards pay the fee"
        );
    }
}
#[test]
fn bp12_007_pays_its_own_nature_card_into_ex_before_damage() {
    for number in printings("BP12-007") {
        let mut initial = position(&number);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"nature","card":"pay-unit"}]);
        let mut game = game(&initial);
        let start = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let (step, q) = until(&mut game, start, 1, true, |d| {
            matches!(d, InputDomain::CostMaterials(_))
        });
        let InputDomain::CostMaterials(domain) = &q.domain else {
            panic!("the requested decision domain is reached")
        };
        assert_eq!(
            domain.members.iter().map(|m| m.player).collect::<Vec<_>>(),
            vec![PlayerId { seat: 1 }],
            "{number}: only the payer's own Nature card is a cost candidate"
        );
        settle(&mut game, step, 1, true);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["ex"],
            json!(["nature"]),
            "CR 10.4.3 limits an unspecified cost field to the payer's field"
        );
        assert_eq!(
            s["P2"]["field"],
            json!(["enemy"]),
            "the opponent's Nature card cannot pay the cost"
        );
        assert_eq!(
            s["objects"]["enemy"]["damage"], 3_i64,
            "the selected follower receives the paid damage"
        );
    }
}
#[test]
fn bp20_096_lastword_pays_one_to_search_another_copy_onto_field() {
    for number in printings("BP20-096") {
        let mut initial = position(&number);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"copy","card":"BP20-096"}]);
        let mut own_field = subjects();
        let Selector::Zone { players, .. } = &mut own_field else {
            panic!("field selector")
        };
        players.relation = PlayerRelation::SelfPlayer;
        players.text_quantifier = TextQuantifier::Implicit;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        let mut game =
            with_selected_producer(&initial, Action::Destroy { objects: own_field }, prepared());
        load_stand_ins(&mut game);
        activate(&mut game, "P1");
        settle(&mut game, CardPlay::Cancelled, 2, true);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["field"],
            json!(["copy"]),
            "the Lastword searches the named amulet onto the field"
        );
        assert_eq!(
            s["P1"]["pp"]["current"], 9_i64,
            "the Lastword mode charges one PP"
        );
    }
}
