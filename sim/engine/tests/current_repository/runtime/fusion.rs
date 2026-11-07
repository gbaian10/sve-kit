#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn finish(board: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
            }
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(1).map(|m| m.handle).collect(),
            )),
            other => panic!("unexpected decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn drain(board: &mut Game, mode: u64) -> usize {
    let mut count = 0_usize;
    loop {
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(question) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending trigger")
        };
        let handle = options[0].handle;
        let play = board
            .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap();
        finish(board, play, mode);
        count = count.checked_add(1).unwrap();
    }
    count
}

fn line(number: &str, printed: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == printed)
        .unwrap()
        .node
        .id
}

// The paired printings share the expensive real repository fixture and exercise all six aliases.
#[test]
fn bp20_026_fuses_then_plays_with_discount_and_notifies_field_and_ex_observers() {
    for (spell, octris, crest) in [
        ("BP20-026", "BP20-019", "BP20-P61"),
        ("PR-551", "BP20-SL05", "BP20-T02"),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":spell}, {"id":"material","card":spell},
            {"id":"wrong","card":"BP01-034"}
        ]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"octris","card":octris}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"crest","card":crest}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-010"}]);
        let mut board = game(&initial);
        let CardPlay::Awaiting(question) = board
            .activate_internal_card("source", line(spell, 2), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("Fusion material")
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("Fusion material domain")
        };
        assert_eq!(
            domain.members.len(),
            1,
            "source itself and non-treasure hand cards are excluded"
        );
        assert!(
            board.internal_decision(View::P2).is_none(),
            "private material question"
        );
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        assert_eq!(
            drain(&mut board, 1),
            2,
            "one Fusion gives one field and one EX occurrence"
        );
        let fused = board.projection(View::Referee).unwrap();
        assert_eq!(fused["P1"]["cemetery"], json!(["material"]));
        assert_eq!(fused["objects"]["octris"]["keywords"], json!(["疾走"]));
        assert_eq!(
            fused["objects"]["crest"]["counters"]["返戻カウンター"],
            1_u8
        );
        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, 1);
        assert_eq!(
            drain(&mut board, 1),
            2,
            "the card Play is separate from Fusion"
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "Fusion grant reduces the actual EX play from two PP to one"
        );
        assert_eq!(
            state["P2"]["cemetery"],
            json!(["enemy"]),
            "the real spell deals four damage"
        );
        assert_eq!(
            state["objects"]["crest"]["counters"]["返戻カウンター"],
            2_u8
        );
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP15-P29"),
            "Octris name condition creates a golden sword"
        );
        assert!(
            board.projection(View::P2).unwrap()["objects"]["wrong"].is_null(),
            "unpaid hand identities remain hidden"
        );
    }
}

#[test]
fn bp20_019_all_prints_choose_crest_or_search_the_named_spell() {
    for number in ["BP20-019", "BP20-SL05"] {
        for mode in [1_u64, 2] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"wanted","card":"PR-551"}, {"id":"wrong","card":"BP01-034"}
            ]);
            let mut board = game(&initial);
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, play, mode);
            assert_eq!(drain(&mut board, mode), 1, "exactly its fanfare");
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if mode == 2 {
                    json!(["wanted"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                usize::from(mode == 1)
            );
            assert_eq!(
                state["P1"]["deck"].as_array().unwrap().len(),
                if mode == 1 { 2 } else { 1 }
            );
        }
    }
}

#[test]
fn bp20_026_uses_an_ex_treasure_token_to_free_a_full_ex_slot() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-026"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("gold{n}"),"card":"BP15-P29"}))
            .collect::<Vec<_>>()
    );
    let mut board = game(&initial);
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("source", line("BP20-026", 2), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("EX Fusion material")
    };
    let result = resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["ex"].as_array().unwrap().len(),
        5,
        "the material's departure makes room before the source enters"
    );
    assert!(
        state["P1"]["ex"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert!(
        state["objects"]["gold0"].is_null(),
        "the paid Spell token is erased after reaching the cemetery"
    );
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Discard(_))),
        "an EX material is not a hand discard"
    );
    assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Fused(event) if event.materials.len() == 1 && event.material_traits.contains(&TraitId("財宝".into())))), "the erased material's traits are retained in the Fusion fact");
}

#[test]
fn bp20_crest_main_phase_reaches_eight_banishes_and_searches_a_thief() {
    // Both printings share the real rule-indexed selection; its first construction is expensive.
    for number in ["BP20-P61", "BP20-T02"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"crest","card":number,"state":{"counters":{"返戻カウンター":7_u8}}}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"reserve","card":"BP01-171"}, {"id":"wanted","card":"BP20-026"},
            {"id":"reserve2","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["zones"]["deck"] = json!([
            {"id":"other-draw","card":"BP01-171"}, {"id":"other-reserve","card":"BP01-171"}
        ]);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "fusion-crest-main",
        )
        .unwrap();
        for turn in 0..2_u8 {
            assert_eq!(
                board
                    .internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
            let mut triggers = 0_usize;
            loop {
                triggers += drain(&mut board, 1);
                let Some(question) = board.internal_quick_choice().unwrap() else {
                    break;
                };
                let pass = board
                    .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                    .unwrap();
                finish(&mut board, pass, 1);
            }
            assert_eq!(
                triggers,
                usize::from(turn == 1),
                "only the controller's main phase triggers the crest"
            );
        }
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["ex"],
            json!([]),
            "eighth counter removes the crest"
        );
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("wanted")),
            "the resolved main-phase body searches a thief"
        );
        assert!(
            state["objects"]["crest"].is_null(),
            "a crest token is erased immediately outside EX"
        );
        assert_eq!(
            state["P1"]["banish"],
            json!([]),
            "the token does not linger in banishment"
        );
    }
}
