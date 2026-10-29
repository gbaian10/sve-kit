#![expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the scenario."
)]
use super::*;
use std::sync::LazyLock;
use sve_engine::game::internal::DecisionView;

fn fixture(initial: &Value) -> Game {
    static SELECTED: LazyLock<SelectedEffects> = LazyLock::new(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, class, kind, traits) in [
            ("filter-elf", "エルフ", "フォロワー", vec![]),
            ("filter-bishop", "ビショップ", "フォロワー", vec![]),
            (
                "filter-amulet",
                "ニュートラル",
                "アミュレット・トークン",
                vec!["人形"],
            ),
            ("filter-doll", "ニュートラル", "フォロワー", vec!["人形"]),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":number,"card_class":class,"card_type":kind,"traits":traits,
                "cost":"0","power":if kind.contains("アミュレット") {"-"} else {"1"},"hp":if kind.contains("アミュレット") {"-"} else {"1"},"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set",
                    "cards":{number:{"status":"complete","abilities":[]}}})
                .to_string()
            ]));
        }
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        SELECTED.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "class-set-filter",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "filter-elf",
        "filter-bishop",
        "filter-amulet",
        "filter-doll",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}
fn accept(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            break;
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending trigger")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}
fn enter(game: &mut Game, card: &str) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(accept(game, play), CardPlay::Entered(_)),
        "the printed follower enters before its trigger"
    );
    trigger(game, card)
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("targets")
    };
    question
}
#[test]
fn bp04_034_class_exists_includes_neutral_and_excludes_royal_and_opponents() {
    for eligible in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP04-034"}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":if eligible{"BP04-117"}else{"BP01-037"}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent","card":"BP04-117"}]);
        let mut game = fixture(&initial);
        let play = enter(&mut game, "BP04-034");
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            5_u8 + u8::from(eligible)
        );
        assert_eq!(state["objects"]["source"]["hp"], 5_u8 + u8::from(eligible));
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            eligible
        );
    }
}
#[test]
fn bp09_101_class_target_excludes_other_classes_and_self() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-101"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"bishop","card":"filter-bishop"},{"id":"royal","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"filter-bishop"}]);
    let mut game = fixture(&initial);
    let question = awaiting(enter(&mut game, "BP09-101"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("filter-bishop"));
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["objects"]["bishop"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("必殺"))
    );
    assert!(
        !state["objects"]["royal"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("必殺"))
    );
}
#[test]
fn bp04_013_fixed_class_set_includes_created_fairy_at_combo_three() {
    for prior in [0_usize, 2] {
        let mut initial = draw_position();
        let mut hand = vec![json!({"id":"source","card":"BP04-013"})];
        for n in 0..prior {
            hand.push(json!({"id":format!("prior{n}"),"card":"filter-elf"}));
        }
        initial["players"]["P1"]["zones"]["hand"] = json!(hand);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"royal","card":"BP01-037"}]);
        let mut game = fixture(&initial);
        for n in 0..prior {
            assert!(matches!(
                game.play_internal_card(&format!("prior{n}"), PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            game.resume_internal_card().unwrap();
        }
        resolved(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["royal"]["power"], 5_u8);
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), prior + 2);
        for id in field {
            if id == "royal" {
                continue;
            }
            let object = &state["objects"][id.as_str().unwrap()];
            assert_eq!(object["power"], if prior == 2 { 2_u8 } else { 1_u8 });
            assert_eq!(object["hp"], if prior == 2 { 2_u8 } else { 1_u8 });
        }
    }
}
#[test]
fn bp19_012_fixed_trait_token_intersection_excludes_non_tokens_and_other_traits() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-012"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"doll","card":"filter-doll"},{"id":"fairy","card":"BP01-T03"},{"id":"amulet","card":"filter-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP08-T09"}]);
    let mut game = fixture(&initial);
    resolved(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    for id in ["doll", "fairy", "enemy"] {
        assert_eq!(state["objects"][id]["power"], 1_u8);
        assert!(
            !state["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃"))
        );
    }
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 5);
    for id in field {
        if id == "doll" || id == "fairy" || id == "amulet" {
            continue;
        }
        let object = &state["objects"][id.as_str().unwrap()];
        assert_eq!(object["power"], 3_u8);
        assert_eq!(object["hp"], 1_u8);
        assert!(
            object["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃"))
        );
        assert!(
            object["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進"))
        );
    }
}
