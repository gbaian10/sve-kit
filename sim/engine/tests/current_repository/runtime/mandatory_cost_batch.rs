#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed mandatory-cost scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const MATERIALS: [&str; 14] = [
    "batch-fairy",
    "batch-aria",
    "batch-lisha",
    "batch-nano",
    "batch-big",
    "batch-idol",
    "batch-lishena",
    "batch-fox",
    "batch-julius",
    "batch-enemy",
    "batch-fake-fairy",
    "batch-small",
    "batch-levion",
    "batch-nano-evolved",
];
// Share repository selection once; synthetic companions isolate each printed material condition.
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot,keywords,mut input) = inputs().clone();
        let definitions = [
            ("フェアリー","フォロワー・トークン","1","ニュートラル",vec!["妖精"]),
            ("フェアリープリンセス・アリア","フォロワー","1","エルフ",vec![]),
            ("スカイセイバー・リーシャ","フォロワー","1","ロイヤル",vec![]),
            ("黄昏の刃・ナノ","フォロワー","1","ロイヤル",vec![]),
            ("material","フォロワー","7","ドラゴン",vec![]),
            ("idol","フォロワー","1","ニュートラル",vec!["アイドル"]),
            ("リーシェナの模造","フォロワー","1","ウィッチ",vec![]),
            ("一ツ尾狐","フォロワー","1","ナイトメア",vec![]),
            ("ユリウスの模造","フォロワー","1","ロイヤル",vec!["レヴィオン"]),
            ("enemy","フォロワー","1","ニュートラル",vec![]),
            ("フェアリー","フォロワー","1","ニュートラル",vec!["妖精"]),
            ("small","フォロワー","6","ドラゴン",vec![]),
            ("levion","フォロワー","1","ロイヤル",vec!["レヴィオン"]),
            ("黄昏の刃・ナノ","フォロワー・エボルヴ","-","ロイヤル",vec![]),
        ];
        for (number,(name,kind,cost,class,traits)) in MATERIALS.into_iter().zip(definitions) {
            if !snapshot.ends_with('\n') { snapshot.push('\n'); }
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_type":kind,"cost":cost,"card_class":class,"traits":traits,"power":"1","hp":"20","text":"","sections":[]}]}).to_string());
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}
fn prepared(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "mandatory-cost-batch",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"batch-nano":tree["cards"]["synthetic-evolution"]});
    let mut programs = lowerer().lower(&tree).unwrap();
    let nano = programs.remove("batch-nano").unwrap();
    for number in MATERIALS {
        game.load_internal_card(
            number,
            if number == "batch-nano" {
                &nano
            } else {
                &blank
            },
        )
        .unwrap();
    }
    game
}
fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    if matches!(number, "BP12-P25" | "BP13-P32") {
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":number}]);
    }
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"batch-enemy"}]);
    initial
}
fn answer(domain: &InputDomain) -> Answer {
    match domain {
        InputDomain::Targets(domain) => Answer::Targets(
            domain
                .members
                .iter()
                .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                .map(|member| member.handle)
                .collect(),
        ),
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
                .take(1)
                .map(|card| card.handle)
                .collect(),
        )),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::Pending(options) => Answer::Pending(options[0].handle),
        InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
        InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
        _ => panic!("unexpected choice"),
    }
}
fn finish(game: &mut Game, play: CardPlay) {
    finish_with_mode(game, play, ModeId(1));
}
fn finish_with_mode(game: &mut Game, mut play: CardPlay, mode: ModeId) {
    loop {
        while let CardPlay::Awaiting(question) = play {
            play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    if matches!(question.domain, InputDomain::Modes { .. }) {
                        Answer::Modes(vec![mode])
                    } else {
                        answer(&question.domain)
                    },
                )
                .unwrap();
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                if matches!(question.domain, InputDomain::Modes { .. }) {
                    Answer::Modes(vec![mode])
                } else {
                    answer(&question.domain)
                },
            )
            .unwrap();
    }
}
fn play(game: &mut Game) {
    let result = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, result);
}

#[test]
fn bp11_007_sacrifices_four_fairy_tokens_before_searching_aria_into_the_field() {
    let mut initial = position("BP11-007");
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..4_u8)
            .map(|i| json!({"id":format!("fairy{i}"),"card":"batch-fairy"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"aria","card":"batch-aria"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["aria"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    for i in 0..4_u8 {
        assert!(
            !state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("fairy{i}")))
        );
    }
}
#[test]
fn bp12_p25_pays_both_distinct_named_posture_groups_before_ten_damage() {
    let mut initial = position("BP12-P25");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lisha","card":"batch-lisha"},{"id":"nano","card":"batch-nano"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["lisha"]["acted"], true);
    assert_eq!(state["objects"]["nano"]["acted"], true);
    assert_eq!(state["objects"]["enemy"]["damage"], 10_u8);
    assert_eq!(state["P2"]["leader"]["life"], 10_u8);
}
#[test]
fn bp13_064_acts_an_original_seven_cost_dragon_then_heals_and_draws_two() {
    let mut initial = position("BP13-064");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"batch-big"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["material"]["acted"], true);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["hand"], json!(["draw1", "draw2"]));
}
#[test]
fn bp13_p32_sacrifices_two_followers_before_leader_damage_and_healing() {
    let mut initial = position("BP13-P32");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"first","card":"batch-idol"},{"id":"second","card":"batch-idol"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("first"))
    );
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("second"))
    );
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
}
#[test]
fn bp15_045_banishes_a_cemetery_lishena_then_searches_another_when_two_idols_exist() {
    let mut initial = position("BP15-045");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"material","card":"batch-lishena"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"idol1","card":"batch-idol"},{"id":"idol2","card":"batch-idol"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"batch-lishena"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["material"]));
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("found"))
    );
}
#[test]
fn bp15_083_pays_two_foxes_for_section_spell_then_uses_cemetery_ability_to_grant_storm() {
    let mut initial = position("BP15-083");
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("fox{i}"),"card":"batch-fox"}))
            .collect::<Vec<_>>()
    );
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["fox2"]));
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P1"]["hand"], json!(["draw1"]));
    let ability = selected().current["BP15-083"]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.source.section.is_none())
        .unwrap()
        .node
        .id;
    let result = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, result);
    let after = game.projection(View::Referee).unwrap();
    assert!(
        after["P1"]["banish"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert!(
        after["objects"]["fox2"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}
#[test]
fn bp16_026_acts_levion_julius_and_draws_only_after_four_damage_and_healing() {
    let mut initial = position("BP16-026");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"batch-julius"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["material"]["acted"], true);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["P1"]["hand"], json!(["draw1"]));
}
#[test]
fn bp12_020_fanfare_searches_named_nano_into_the_field() {
    let mut initial = position("BP12-020");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"nano","card":"batch-nano"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("nano"))
    );
}
#[test]
fn bp13_075_evolution_creates_the_newly_admitted_immoral_desire_token() {
    let mut initial = position("BP13-074");
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-074"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-075"}]);
    let mut game = prepared(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let result = game
        .answer_internal_card(question.actor, question.token, answer(&question.domain))
        .unwrap();
    finish(&mut game, result);
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(
        state["objects"][ex[0].as_str().unwrap()]["name"],
        "インモラルディザイア"
    );
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("必殺"))
    );
    assert_eq!(state["objects"]["source"]["evolved_with"], "form");
}

#[test]
fn bp12_020_nano_evolution_can_create_twilight_sword_or_heal() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = position("BP12-020");
        initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP12-020"},{"id":"nano","card":"batch-nano"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"batch-nano-evolved"}]);
        let mut game = prepared(&initial);
        // A minimal named companion produces the real trigger without admitting another card.
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"nano","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let question = game.internal_pending_choice().unwrap().unwrap();
        let play = game
            .answer_internal_card(question.actor, question.token, answer(&question.domain))
            .unwrap();
        finish_with_mode(&mut game, play, mode);
        let state = game.projection(View::Referee).unwrap();
        let ex = state["P1"]["ex"].as_array().unwrap();
        if mode == ModeId(1) {
            assert_eq!(ex.len(), 1);
            assert_eq!(
                state["objects"][ex[0].as_str().unwrap()]["name"],
                "トワイライトソード"
            );
            assert_eq!(state["P1"]["leader"]["life"], 10_u8);
        } else {
            assert_eq!(ex, &Vec::<Value>::new());
            assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        }
    }
}
#[test]
fn bp16_026_without_julius_heals_and_damages_without_drawing() {
    let mut initial = position("BP16-026");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"batch-levion"}]);
    let mut game = prepared(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}
#[test]
fn missing_or_wrong_mandatory_materials_reject_play_without_mutating_the_game() {
    for (number, material, count) in [
        ("BP11-007", "batch-fake-fairy", 4_u8),
        ("BP12-P25", "batch-lisha", 1),
        ("BP13-064", "batch-small", 1),
        ("BP13-P32", "batch-idol", 1),
        ("BP15-045", "batch-lishena", 0),
        ("BP15-083", "batch-fox", 1),
        ("BP16-026", "batch-idol", 1),
    ] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("material{i}"),"card":material}))
                .collect::<Vec<_>>()
        );
        let mut game = prepared(&initial);
        let before = game.projection(View::Referee).unwrap();
        let mut attempt = game.play_internal_card("source", PlayerId { seat: 1 });
        loop {
            match attempt {
                Err(EngineFailure::IllegalPlay(_)) => break,
                Ok(CardPlay::Awaiting(question)) => {
                    attempt = game.answer_internal_card(
                        question.actor,
                        question.token,
                        answer(&question.domain),
                    );
                }
                _ => panic!("mandatory payment unexpectedly accepted {number}: {attempt:?}"),
            }
        }
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}
