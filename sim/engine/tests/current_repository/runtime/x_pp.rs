#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed X payment scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

type InertCard = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const INERT_CARDS: &[InertCard] = &[
    ("x-neutral", "1", "フォロワー", &[]),
    ("x-academy-low", "2", "フォロワー", &["学院"]),
    ("x-academy-high", "3", "フォロワー", &["学院"]),
    ("x-academy-spell", "1", "スペル", &["学院"]),
    ("x-soldier", "2", "フォロワー", &["兵士"]),
    ("x-soldier-high", "3", "フォロワー", &["兵士"]),
    ("x-soldier-spell", "1", "スペル", &["兵士"]),
    ("x-onmyoji", "2", "フォロワー", &["陰陽師"]),
    ("x-onmyoji-spell", "1", "スペル", &["陰陽師"]),
    ("x-other-spell", "1", "スペル", &[]),
    ("x-priconne", "2", "フォロワー", &["プリコネ"]),
    ("x-priconne-high", "3", "フォロワー", &["プリコネ"]),
    ("x-priconne-spell", "1", "スペル", &["プリコネ"]),
];

// Small inert followers isolate X limits from the searched or revived cards' own text.
fn prepared() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for &(number, cost, kind, traits) in INERT_CARDS {
            if !snapshot.ends_with('\n') { snapshot.push('\n'); }
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"traits":traits,"cost":cost,"power":if kind=="フォロワー" {"2"} else {"-"},"hp":if kind=="フォロワー" {"8"} else {"-"},"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn board(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "X-PP-current",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for &(number, _, _, _) in INERT_CARDS {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}
fn position(number: &str, zone: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"x-neutral"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"x-neutral"}]);
    initial
}
fn finish(game: &mut Game, play: CardPlay, x: u64) {
    finish_search(game, play, x, false);
}
fn finish_search(game: &mut Game, mut play: CardPlay, x: u64, skip_search: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Integer(bounds) => {
                assert_eq!(bounds.minimum, Nat(0), "zero X remains legal");
                assert!(
                    bounds.maximum.is_some_and(|maximum| x <= maximum.0),
                    "the requested X fits all remaining PP payments"
                );
                Answer::Integer(Nat(x))
            }
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the follower within X and the printed filters is eligible"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "only the printed follower filter and X cost limit match"
                );
                Answer::Search(SearchAnswer::Ungrouped(if skip_search {
                    vec![]
                } else {
                    vec![domain.cards[0].handle]
                }))
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    2,
                    "both additional Onmyoji hand cards must be revealed"
                );
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "hand material choices remain private until payment"
                );
                Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect())
            }
            _ => panic!("unexpected X question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    if !matches!(play, CardPlay::Entered(_)) {
        resolved(play);
    }
}
fn settle(game: &mut Game, x: u64) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending trigger");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, x);
    }
}

#[test]
fn bp09_116_x_pp_fanfare_buffs_by_the_declared_amount_including_zero() {
    for x in [0_u8, 4] {
        let pp = x + 1;
        let mut game = board(&position("BP09-116", "hand", pp));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, u64::from(x));
        settle(&mut game, u64::from(x));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "printed play cost and X are distinct payments"
        );
        assert_eq!(view["objects"]["source"]["power"], 2 + x);
        assert_eq!(view["objects"]["source"]["hp"], 1 + x);
    }
}

#[test]
fn bp06_099_x_pp_damages_every_enemy_and_keeps_its_end_phase_recovery_for_all_prints() {
    for number in ["BP06-099", "PR-135"] {
        let mut initial = position(number, "hand", 8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"x-neutral"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fragile","card":"x-neutral","state":{"hp":3_u8,"max_hp":3_u8}}));
        let mut game = board(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, 3);
        settle(&mut game, 3);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 3_u8);
        assert_eq!(view["objects"]["enemy"]["hp"], 5_u8);
        assert_eq!(view["objects"]["friend"]["hp"], 8_u8);
        assert_eq!(view["P2"]["cemetery"], json!(["fragile"]));
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut game, 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            4_u8,
            "the independent end-phase ability recovers one PP"
        );
    }
}

#[test]
fn bp21_030_x_pp_revives_only_another_named_academy_follower_within_x_for_all_prints() {
    for number in ["BP21-030", "BP21-P15"] {
        let mut initial = position(number, "field", 5);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"low","card":"x-academy-low"},{"id":"high","card":"x-academy-high"},
            {"id":"same","card":"BP21-030"},{"id":"wrong","card":"x-neutral"},
            {"id":"spell","card":"x-academy-spell"}
        ]);
        let mut game = board(&initial);
        let source = game.internal_activated_sources("source").unwrap().remove(0);
        let play = game
            .activate_internal_card("source", source.definition, PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, 2);
        settle(&mut game, 0);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 3_u8);
        assert_eq!(view["objects"]["source"]["acted"], true);
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("low"))
        );
        for id in ["high", "same", "wrong", "spell"] {
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id)),
                "cost, trait, follower kind and same-name exclusions remain enforced"
            );
        }
    }
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected printed X declaration");
    };
    question
}

#[test]
fn bp04_021_x_pp_evolution_searches_a_soldier_and_its_entry_ability_adds_one_power_for_all_prints()
{
    for number in ["BP04-021", "BP04-SL05"] {
        let mut initial = position("BP04-020", "field", 3);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"soldier","card":"x-soldier"},{"id":"high","card":"x-soldier-high"},
            {"id":"spell","card":"x-soldier-spell"},{"id":"wrong","card":"x-neutral"}
        ]);
        let mut game = board(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base Mars evolves for zero printed PP"
        );
        settle(&mut game, 2);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 1_u8);
        assert_eq!(
            view["objects"]["soldier"]["power"], 3_u8,
            "the evolved Mars observes the searched soldier entering"
        );
        assert_eq!(view["objects"]["soldier"]["hp"], 8_u8);
        for id in ["high", "spell", "wrong"] {
            assert!(
                view["P1"]["deck"].as_array().unwrap().contains(&json!(id)),
                "the X search excludes higher costs, other kinds and other traits"
            );
        }
    }
}

#[test]
fn bp15_046_x_pp_reveals_two_onmyoji_then_searches_and_counts_the_grave_union_once() {
    for number in ["BP15-046", "BP15-U03"] {
        for (grave, accepted, found) in [
            (6_u8, true, true),
            (7, true, true),
            (7, true, false),
            (7, false, false),
        ] {
            let mut initial = position(number, "hand", 4);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"hand-follower","card":"x-onmyoji"}),
                    json!({"id":"hand-spell","card":"x-onmyoji-spell"}),
                ]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"found","card":"x-onmyoji"},{"id":"spell","card":"x-onmyoji-spell"},{"id":"wrong","card":"x-neutral"}
            ]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!((0..grave).map(|i| json!({"id":format!("grave{i}"),"card":if i==0 {"x-onmyoji-spell"} else if i%2==0 {"x-other-spell"} else {"x-onmyoji"}})).collect::<Vec<_>>());
            let mut game = board(&initial);
            let declare = question(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            assert_eq!(
                declare.domain,
                InputDomain::OptionalCost,
                "decide the spell's optional X payment before choosing hand materials"
            );
            let play = game
                .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accepted))
                .unwrap();
            if accepted {
                assert!(
                    matches!(&play, CardPlay::Awaiting(q) if matches!(q.domain, InputDomain::Integer(_))),
                    "declare X before the additional reveal materials"
                );
            }
            finish_search(&mut game, play, 2, !found);
            settle(&mut game, 0);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], if accepted { 2_u8 } else { 4 });
            assert_eq!(
                view["P1"]["hand"].as_array().unwrap().len(),
                2,
                "revelation does not discard either required card"
            );
            assert_eq!(
                view["P1"]["field"].as_array().map_or(0, Vec::len),
                usize::from(accepted && found)
            );
            let ex = view["P1"]["ex"].as_array().map_or(&[][..], Vec::as_slice);
            assert_eq!(
                ex.len(),
                usize::from(accepted && grave == 7),
                "Onmyoji spells count once and the resolving spell is not yet in the grave"
            );
            if accepted && grave == 7 {
                let token = ex[0].as_str().unwrap();
                let token_play = game
                    .play_internal_card(token, PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, token_play, 0);
                settle(&mut game, 0);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    1_u8,
                    "the created two-cost token permanently gains minus one play cost"
                );
            }
        }
    }
}

#[test]
fn cp04_096_x_pp_ub_evolution_revives_priconne_and_heals_only_for_another_followers_ub_for_all_prints()
 {
    for number in ["CP04-096", "CP04-P72", "PR-500"] {
        let mut initial = position("CP04-095", "field", 5);
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other-ub","card":"CP04-022"}));
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"revived","card":"x-priconne"},{"id":"high","card":"x-priconne-high"},
            {"id":"wrong","card":"x-neutral"},{"id":"spell","card":"x-priconne-spell"}
        ]);
        let mut game = board(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        settle(&mut game, 2);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 2_u8);
        assert_eq!(
            view["P1"]["leader"]["life"], 10_u8,
            "Yui does not observe her own UB as another follower"
        );
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
        for id in ["high", "wrong", "spell"] {
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id)),
                "the revival respects X, trait and follower kind"
            );
        }
        let other = game
            .internal_activated_sources("other-ub")
            .unwrap()
            .remove(0);
        let play = game
            .activate_internal_card("other-ub", other.definition, PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, 0);
        settle(&mut game, 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8,
            "another real follower's UB produces Yui's separate life trigger"
        );
    }
}
