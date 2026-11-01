#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed conditional card scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, &[&str]); 13] = [
    ("cute", "フォロワー", "ニュートラル", &["キュート"]),
    ("cool", "フォロワー", "ニュートラル", &["クール"]),
    ("passion", "フォロワー", "ニュートラル", &["パッション"]),
    ("hachigoku", "スペル", "ニュートラル", &["八獄"]),
    ("arcana", "フォロワー", "ニュートラル", &["アルカナ"]),
    ("banquet", "フォロワー", "ニュートラル", &["宴楽"]),
    ("banquet-amulet", "アミュレット", "ニュートラル", &["宴楽"]),
    ("royal-follower", "フォロワー", "ロイヤル", &["兵士"]),
    ("royal-spell", "スペル", "ロイヤル", &["兵士"]),
    ("elf-follower", "フォロワー", "エルフ", &["死者"]),
    ("elf-spell", "スペル", "エルフ", &["死者"]),
    ("plain-follower", "フォロワー", "ニュートラル", &["兵士"]),
    ("plain-spell", "スペル", "ニュートラル", &["兵士"]),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, kind, class, traits) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":number,"card_class":class,"card_type":kind,
                "traits":traits,"cost":"1","power":if follower {"1"} else {"-"},
                "hp":if follower {"9"} else {"-"},"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules:Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot,json!([]),json!([]))["input"].clone();
        Catalog::from_current_selection_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap()
    })
}

pub(super) fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "choice-conditions",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

pub(super) fn position(number: &str, seat: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["leader"]["life"] = json!(10_u8);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"][seat]["zones"]["deck"] = json!([
        {"id":"first","card":"plain-spell"},{"id":"second","card":"plain-spell"},
        {"id":"third","card":"plain-spell"}
    ]);
    initial
}

pub(super) fn enter(game: &mut Game, seat: &str) {
    let mut step = game
        .play_internal_card(
            "source",
            PlayerId {
                seat: seat_number(seat),
            },
        )
        .unwrap();
    if let CardPlay::Awaiting(ward) = step {
        assert!(
            matches!(ward.domain, InputDomain::WardEntry { .. }),
            "only the printed Ward entry choice precedes the trigger"
        );
        step = game
            .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(step, CardPlay::Entered(_)),
        "the real card enters before its fanfare"
    );
}

pub(super) fn trigger(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending")
    };
    assert_eq!(options.len(), 1, "only this printed ability is pending");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

pub(super) fn awaiting(step: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = step else {
        panic!("expected payment")
    };
    question
}

pub(super) fn keyword(state: &Value, name: &str) -> bool {
    state["objects"]["source"]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}

#[test]
fn bp18_023_and_sl08_count_only_royal_followers_and_heal_at_every_threshold() {
    for number in ["BP18-023", "BP18-SL08"] {
        for count in [0_usize, 5, 10, 15, 20] {
            let mut initial = position(number, "P1");
            let mut cemetery = (0..count)
                .map(|index| json!({"id":format!("royal-{index}"),"card":"royal-follower"}))
                .collect::<Vec<_>>();
            cemetery.extend([
                json!({"id":"wrong-kind","card":"royal-spell"}),
                json!({"id":"wrong-class","card":"elf-follower"}),
            ]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
            let mut game = fixture(&initial);
            let before = game.projection(View::Referee).unwrap();
            enter(&mut game, "P1");
            resolved(trigger(&mut game));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["leader"]["life"], 12_u8);
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(count >= 5)
            );
            for (name, threshold) in [("疾走", 10_usize), ("威圧", 15), ("オーラ", 15)] {
                assert_eq!(keyword(&state, name), count >= threshold);
            }
            let gain = if count >= 20 { 7 } else { 0 };
            assert_eq!(
                state["objects"]["source"]["power"].as_i64().unwrap(),
                before["objects"]["source"]["power"].as_i64().unwrap() + gain
            );
            assert_eq!(
                state["objects"]["source"]["hp"].as_i64().unwrap(),
                before["objects"]["source"]["hp"].as_i64().unwrap() + gain
            );
        }
    }
}

#[test]
fn bp19_112_and_all_printings_draw_and_heal_independently_of_sep() {
    for number in ["BP19-112", "BP19-SL28", "PCS02-003"] {
        for seat in ["P1", "P2"] {
            for sep in [0_u8, 1] {
                let mut initial = position(number, seat);
                initial["players"][seat]["sep"] = json!(sep);
                let mut game = fixture(&initial);
                let before = game.projection(View::Referee).unwrap();
                enter(&mut game, seat);
                resolved(trigger(&mut game));
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state[seat]["leader"]["life"], 12_u8);
                assert_eq!(state[seat]["hand"], json!(["first"]));
                let gain = if sep == 0 { 2 } else { 0 };
                assert_eq!(
                    state["objects"]["source"]["power"].as_i64().unwrap(),
                    before["objects"]["source"]["power"].as_i64().unwrap() + gain
                );
                assert_eq!(
                    state["objects"]["source"]["hp"].as_i64().unwrap(),
                    before["objects"]["source"]["hp"].as_i64().unwrap() + gain
                );
            }
        }
    }
}

#[test]
fn bp20_114_and_promo_gain_stats_and_storm_only_at_zero_sep() {
    for number in ["BP20-114", "PR-555"] {
        for sep in [0_u8, 1] {
            let mut initial = position(number, "P1");
            initial["players"]["P1"]["sep"] = json!(sep);
            let mut game = fixture(&initial);
            let before = game.projection(View::Referee).unwrap();
            enter(&mut game, "P1");
            resolved(trigger(&mut game));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(keyword(&state, "疾走"), sep == 0);
            let gain = i64::from(sep == 0);
            assert_eq!(
                state["objects"]["source"]["power"].as_i64().unwrap(),
                before["objects"]["source"]["power"].as_i64().unwrap() + gain
            );
            assert_eq!(
                state["objects"]["source"]["hp"].as_i64().unwrap(),
                before["objects"]["source"]["hp"].as_i64().unwrap() + gain
            );
        }
    }
}

pub(super) fn paid(number: &str, card: &str) -> Value {
    paid_for(number, card, "P1", None)
}

fn paid_for(number: &str, card: &str, seat: &str, exclude: Option<&str>) -> Value {
    let mut initial = position(number, seat);
    initial["players"][seat]["zones"]["hand"] =
        json!([{"id":"source","card":number},{"id":"payment","card":card}]);
    if let Some(excluded) = exclude {
        initial["players"][seat]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"excluded","card":excluded}));
    }
    let mut game = fixture(&initial);
    enter(&mut game, seat);
    let mut payment = awaiting(trigger(&mut game));
    if matches!(payment.domain, InputDomain::OptionalCost) {
        payment = awaiting(
            game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
                .unwrap(),
        );
    }
    let InputDomain::CostMaterials(domain) = &payment.domain else {
        panic!("discard payment")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the eligible own hand card is a payment candidate"
    );
    assert!(
        game.internal_decision(if seat == "P1" { View::P2 } else { View::P1 })
            .is_none(),
        "the opponent cannot see a private hand cost choice"
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(payment.actor, payment.token, Answer::CostMaterials(vec![]))
        .unwrap_err();
    assert!(
        game.projection(View::Referee).unwrap() == before,
        "an incomplete required hand cost cannot mutate the game"
    );
    resolved(
        game.answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp09_015_requires_both_elf_class_and_spell_kind_in_the_paid_before_view() {
    for (card, draw) in [
        ("elf-spell", true),
        ("elf-follower", false),
        ("royal-spell", false),
    ] {
        let state = paid("BP09-015", card);
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["payment"]));
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            usize::from(draw)
        );
    }
}

#[test]
fn cp02_056_and_p44_use_actual_discarded_traits_once_each_even_when_zero_cards_are_paid() {
    for number in ["CP02-056", "CP02-P44"] {
        for selected in [
            vec![],
            vec!["cute"],
            vec!["cool"],
            vec!["passion"],
            vec!["cute", "cool", "passion"],
        ] {
            let mut initial = position(number, "P1");
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"cute-payment","card":"cute"}),
                    json!({"id":"cool-payment","card":"cool"}),
                    json!({"id":"passion-payment","card":"passion"}),
                ]);
            let mut game = fixture(&initial);
            enter(&mut game, "P1");
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("two fanfares")
            };
            assert_eq!(options.len(), 2);
            let first = options
                .iter()
                .find(|option| option.line == 1)
                .unwrap()
                .handle;
            let mut payment = awaiting(
                game.answer_internal_card(pending.actor, pending.token, Answer::Pending(first))
                    .unwrap(),
            );
            if matches!(payment.domain, InputDomain::OptionalCost) {
                payment = awaiting(
                    game.answer_internal_card(
                        payment.actor,
                        payment.token,
                        Answer::OptionalCost(true),
                    )
                    .unwrap(),
                );
            }
            let InputDomain::CostMaterials(domain) = &payment.domain else {
                panic!("up-to-three hand cost")
            };
            assert_eq!(domain.cardinality.minimum, Nat(0));
            assert_eq!(domain.cardinality.maximum, Some(Nat(3)));
            let handles = domain
                .members
                .iter()
                .filter(|member| selected.contains(&member.card.as_deref().unwrap()))
                .map(|member| member.handle)
                .collect();
            resolved(
                game.answer_internal_card(
                    payment.actor,
                    payment.token,
                    Answer::CostMaterials(handles),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["cemetery"].as_array().unwrap().len(),
                selected.len()
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if selected.contains(&"cute") {
                    12_u8
                } else {
                    10
                }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if selected.contains(&"passion") {
                    18_u8
                } else {
                    20
                }
            );
            let expected = 3 - selected.len() + usize::from(selected.contains(&"cool"));
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), expected);
            game.resume_internal_card().unwrap();
            resolved(trigger(&mut game));
            let after_second = game.projection(View::Referee).unwrap();
            assert_eq!(
                after_second["P1"]["hand"].as_array().unwrap().len(),
                expected + 2
            );
        }
    }
}

#[test]
fn bp17_033_draws_two_only_for_the_actual_royal_spell_payment() {
    for seat in ["P1", "P2"] {
        for (card, count) in [("royal-spell", 2_usize), ("royal-follower", 1)] {
            let state = paid_for("BP17-033", card, seat, Some("elf-spell"));
            assert_eq!(state[seat]["cemetery"], json!(["payment"]));
            assert_eq!(state[seat]["hand"].as_array().unwrap().len(), count + 1);
            assert!(
                state[seat]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("excluded"))
            );
        }
    }
}
