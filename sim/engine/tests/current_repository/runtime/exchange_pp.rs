#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed PP exchange scenarios."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::SearchAnswer;

// Inert named followers isolate the spells from the searched cards' own abilities.
fn prepared() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name) in [
            ("pp-maim", "レヴィオンの副団長・マイム"),
            ("pp-miim", "レヴィオンの魔術師・ミイム"),
            ("pp-meim", "レヴィオンの武闘家・メイム"),
            ("pp-drache", "焦熱の竜翼・ドラーク"),
            ("pp-dummy", "unmatched"),
        ] {
            if !snapshot.ends_with('\n') { snapshot.push('\n'); }
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","traits":[],"cost":"1","power":"1","hp":"10","text":null,"sections":[]}]}).to_string());
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
        "pp-exchange",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["pp-maim", "pp-miim", "pp-meim", "pp-drache", "pp-dummy"] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}
fn position(number: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"maim","card":"pp-maim"},{"id":"maim2","card":"pp-maim"},
        {"id":"miim","card":"pp-miim"},{"id":"meim","card":"pp-meim"},
        {"id":"drache","card":"pp-drache"},{"id":"wrong","card":"pp-dummy"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"pp-dummy"}]);
    initial
}
fn finish(game: &mut Game, mut play: CardPlay, accepted: bool, skip_last: bool) {
    let mut first = true;
    while let CardPlay::Awaiting(question) = play {
        if first {
            assert_eq!(
                question.domain,
                InputDomain::OptionalCost,
                "declare the increase before targets"
            );
            first = false;
        }
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accepted),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "the sole enemy follower is the required target"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "deck choices are private"
                );
                Answer::Search(domain.groups.as_ref().map_or_else(
                    || {
                        SearchAnswer::Ungrouped(
                            domain
                                .cards
                                .iter()
                                .take(1)
                                .map(|card| card.handle)
                                .collect(),
                        )
                    },
                    |groups| {
                        SearchAnswer::Grouped(
                            groups
                                .iter()
                                .enumerate()
                                .map(|(index, group)| {
                                    if skip_last && index == 2 {
                                        vec![]
                                    } else {
                                        group.handles.iter().take(1).copied().collect()
                                    }
                                })
                                .collect(),
                        )
                    },
                ))
            }
            _ => panic!("unexpected PP exchange question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "the complete spell resolves"
    );
}
#[test]
fn bp13_035_pp_exchange_searches_one_sister_or_one_of_each_for_every_printing() {
    for number in ["BP13-035", "SD07-021"] {
        for (accepted, skip_last) in [(false, false), (true, false), (true, true)] {
            let pp = if accepted { 5 } else { 1 };
            let mut game = board(&position(number, pp));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, accepted, skip_last);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 0_u8);
            if accepted {
                let field = view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap())
                    .collect::<BTreeSet<_>>();
                let expected = if skip_last {
                    vec!["maim", "miim"]
                } else {
                    vec!["maim", "miim", "meim"]
                };
                assert_eq!(
                    field,
                    expected.into_iter().collect(),
                    "each requested sister enters, regardless of field ordering"
                );
                assert_eq!(view["P1"]["hand"], json!([]));
            } else {
                assert_eq!(view["P1"]["hand"], json!(["maim"]));
                assert_eq!(view["P1"]["field"], json!([]));
            }
            assert!(
                view["P1"]["deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("maim2")),
                "one copy per name, not any three sisters"
            );
            assert!(
                view["P1"]["deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("wrong"))
            );
        }
    }
}
#[test]
fn bp13_035_grouped_search_offers_ward_posture_for_bp13_032() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(5_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-035"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"maim","card":"BP13-030"},
        {"id":"miim","card":"BP13-032"},
        {"id":"meim","card":"BP13-033"}
    ]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(cost) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("expected the optional PP increase");
    };
    assert_eq!(cost.domain, InputDomain::OptionalCost);
    let CardPlay::Awaiting(search) = game
        .answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("expected the paid spell's grouped search");
    };
    let InputDomain::Search(domain) = &search.domain else {
        panic!("expected search candidates");
    };
    let selected = domain
        .groups
        .as_ref()
        .unwrap()
        .iter()
        .map(|group| vec![group.handles[0]])
        .collect();
    let CardPlay::Awaiting(ward) = game
        .answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Grouped(selected)),
        )
        .unwrap()
    else {
        panic!("expected BP13-032 to offer its Ward entry choice");
    };
    assert_eq!(ward.actor, actor());
    assert!(matches!(
        &ward.domain,
        InputDomain::WardEntry { card, zone, .. } if card == "BP13-032" && zone == "deck"
    ));

    for acted in [false, true] {
        let mut game = game.clone();
        let resolved = game
            .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(acted))
            .unwrap();
        assert!(matches!(resolved, CardPlay::Resolved(_)));
        let view = game.projection(View::Referee).unwrap();
        let field = view["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 3);
        for member in ["maim", "miim", "meim"] {
            assert!(field.contains(&json!(member)));
            assert_eq!(view["objects"][member]["acted"], acted && member == "miim");
        }
    }
}

#[test]
fn bp13_060_pp_exchange_damages_the_target_and_grants_assail_only_after_an_increase() {
    for number in ["BP13-060", "PR-361"] {
        for accepted in [false, true] {
            let pp = if accepted { 5 } else { 2 };
            let mut game = board(&position(number, pp));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, accepted, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 0_u8);
            assert_eq!(view["objects"]["enemy"]["hp"], 7_u8);
            assert_eq!(
                view["P1"]["field"],
                if accepted {
                    json!(["drache"])
                } else {
                    json!([])
                }
            );
            if accepted {
                assert!(
                    view["objects"]["drache"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("指定攻撃")),
                    "only the actual field entry gains Assail"
                );
            }
        }
    }
}

#[test]
fn bp13_060_pp_exchange_cannot_play_without_an_enemy_follower() {
    for number in ["BP13-060", "PR-361"] {
        for accepted in [false, true] {
            let mut initial = position(number, 5);
            initial["players"]["P2"]["zones"]["field"] = json!([]);
            let mut game = board(&initial);
            let before = game.projection(View::Referee).unwrap();
            let play = game.play_internal_card("source", PlayerId { seat: 1 });
            let rejected = match play {
                Ok(CardPlay::Awaiting(question)) => {
                    assert_eq!(question.domain, InputDomain::OptionalCost);
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::OptionalCost(accepted),
                    )
                }
                other => other,
            };
            assert!(
                rejected.is_err() || matches!(rejected, Ok(CardPlay::Cancelled)),
                "the required enemy target prevents the spell from being played"
            );
            let after = game.projection(View::Referee).unwrap();
            for key in ["P1", "P2", "objects"] {
                assert_eq!(
                    after[key], before[key],
                    "no target means no committed payment or card movement"
                );
            }
        }
    }
}
