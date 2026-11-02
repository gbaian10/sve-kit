#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate printed mode payment scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn prepared() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (id, traits) in [("mode-angel", vec!["天使"]), ("mode-fallen", vec!["堕天使"]), ("mode-machine", vec!["機械"])] {
            snapshot.push_str(&json!({"number":id,"faces":[{"name":id,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"3","power":"1","hp":"10","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{id}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn printings(number: &str) -> Vec<String> {
    let id = &prepared().current[number].card_id;
    prepared()
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == id)
        .map(|(printing, _)| printing.clone())
        .collect()
}

fn game(value: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "mode-material",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for id in ["mode-angel", "mode-fallen", "mode-machine"] {
        game.load_internal_card(id, &empty).unwrap();
    }
    game
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"mode-machine"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"mode-machine"}]);
    initial
}

#[expect(
    clippy::panic_in_result_fn,
    clippy::unwrap_in_result,
    reason = "Only engine answers return errors; malformed test questions must fail immediately."
)]
fn finish(
    game: &mut Game,
    mut play: CardPlay,
    mode: u64,
    accepted: bool,
) -> Result<(), EngineFailure> {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::OptionalCost => Answer::OptionalCost(accepted),
            InputDomain::CostMaterials(domain) => {
                let private = domain.members.iter().any(|member| {
                    member
                        .card
                        .as_deref()
                        .is_some_and(|number| number.starts_with("mode-"))
                });
                if private {
                    assert!(
                        game.internal_decision(View::P2).is_none(),
                        "hand cost choices stay private"
                    );
                }
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|card| card.handle)
                    .into_iter()
                    .collect(),
            )),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected mode material question"),
        };
        play = game.answer_internal_card(question.actor, question.token, answer)?;
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "the selected mode completes one formal play"
    );
    Ok(())
}

#[test]
fn bp03_117_discards_only_the_selected_trait_or_declines_without_materials() {
    for printing in printings("BP03-117") {
        for (mode, accepted) in [(1_u64, true), (2, true), (2, false)] {
            let mut initial = position(&printing);
            if accepted {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .extend([
                        json!({"id":"angel","card":"mode-angel"}),
                        json!({"id":"fallen","card":"mode-fallen"}),
                    ]);
            } else {
                initial["players"]["P1"]["pp"] = json!({"current":1_i64,"max":1_i64});
            }
            let mut game = game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, mode, accepted).unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if accepted { 9_i64 } else { 0_i64 },
                "Q551 permits declining the fee while paying the printed one PP"
            );
            assert_eq!(
                state["P2"]["field"],
                if mode == 1 {
                    json!([])
                } else {
                    json!(["enemy"])
                },
                "only the angel mode destroys its preselected target"
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if accepted && mode == 2 {
                    23_i64
                } else {
                    20_i64
                },
                "declining suppresses the whole paid continuation"
            );
            if accepted {
                assert!(
                    state["P1"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(if mode == 1 { "angel" } else { "fallen" })),
                    "the chosen trait supplies the material"
                );
                assert!(
                    !state["P1"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(if mode == 1 { "fallen" } else { "angel" })),
                    "the unselected fee consumes no card"
                );
            } else {
                assert_eq!(
                    state["P1"]["hand"],
                    json!([]),
                    "declining also suppresses drawing"
                );
            }
        }
    }
}

#[test]
fn bp07_092_banishes_two_or_five_ex_repairs_before_search_and_rolls_back_shortage() {
    for printing in printings("BP07-092") {
        for (mode, count, accepted) in [
            (1_u64, 2_usize, true),
            (2, 5, true),
            (2, 0, false),
            (1, 1, true),
        ] {
            let mut initial = position(&printing);
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..count)
                    .map(|index| json!({"id":format!("repair-{index}"),"card":"BP07-T02"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            let before = game.projection(View::Referee).unwrap();
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            let result = finish(&mut game, play, mode, accepted);
            let state = game.projection(View::Referee).unwrap();
            if count == 1 {
                result.unwrap_err();
                assert_eq!(
                    state["P1"], before["P1"],
                    "an accepted but unpayable EX fee restores the entire spell attempt"
                );
            } else {
                result.unwrap();
                assert_eq!(
                    state["P1"]["ex"],
                    json!([]),
                    "only selected Repair Modes are banished"
                );
                assert_eq!(
                    state["P1"]["field"],
                    if accepted {
                        json!(["drawn"])
                    } else {
                        json!([])
                    },
                    "the deck search requires successful payment"
                );
                assert_eq!(
                    state["P1"]["pp"]["current"], 8_i64,
                    "material fees do not replace the printed spell cost"
                );
            }
        }
    }
}

#[test]
fn ecp02_038_optional_mode_pays_four_and_sacrifices_before_searching_into_a_full_field() {
    for printing in printings("ECP02-038") {
        for accepted in [true, false] {
            let mut initial = position("ECP02-037");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            let mut field = vec![json!({"id":"source","card":"ECP02-037"})];
            field.extend(
                (0..4_u8).map(|index| json!({"id":format!("ally-{index}"),"card":"mode-machine"})),
            );
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"ECP02-037"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":printing}]);
            let mut game = game(&initial);
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("evolution trigger")
            };
            let play = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            finish(&mut game, play, 2, accepted).unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if accepted { 5_i64 } else { 9_i64 },
                "the mode charges four in addition to the ordinary evolution fee"
            );
            assert_eq!(
                state["P1"]["field"].as_array().unwrap().len(),
                5_usize,
                "the source sacrifice frees the search destination"
            );
            assert_eq!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("found")),
                accepted,
                "declining neither sacrifices nor searches"
            );
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                accepted,
                "the same evolved source pays the fee"
            );
        }
    }
}
