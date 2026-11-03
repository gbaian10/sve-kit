#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these printed material scenarios."
)]
use super::*;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP16-057" | "BP16-058" | "BP04-116" | "CP02-T01" | "BP03-T02")));
        for (number, class, cost) in [("read-dragon", "ドラゴン", "7"), ("read-low", "ドラゴン", "6"), ("read-other", "ニュートラル", "7")] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":class,"card_type":"フォロワー","traits":[],"cost":cost,"power":"1","hp":"20","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-read",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["read-dragon", "read-low", "read-other"] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"fuel","card":"read-other","state":{"cost":9_u8}}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"read-other"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"read-other"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, accepted: bool) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accepted),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.members.len(), 1, "only the fee remains in hand");
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "unpaid hand candidates stay private"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected payment question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn pending(game: &mut Game, accepted: bool) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("trigger")
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play, accepted)
}

#[test]
fn bp16_057_all_printings_discount_by_original_dragon_cost_and_damage_by_discarded_effective_cost()
{
    for (printing, dragons, filler, accepted, discount) in [
        ("BP16-057", 4_u8, "read-other", true, true),
        ("BP16-SL15", 3_u8, "read-other", true, false),
        ("PR-587", 3_u8, "read-low", false, false),
    ] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"id":"source","card":printing}));
        let mut cemetery: Vec<_> = (0_u8..dragons).map(|index| json!({"id":format!("grave-{index}"),"card":"read-dragon","state":{"cost":0_u8}})).collect();
        cemetery.push(json!({"id":"wrong","card":filler,"state":{"cost":9_u8}}));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
        let mut board = game(&initial);
        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(play, CardPlay::Entered(_)),
            "the complete base enters normally"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if discount { 5_u8 } else { 3_u8 },
            "only four original seven-cost Dragon cards reduce the fee"
        );
        let trigger_play = pending(&mut board, accepted);
        if accepted {
            resolved(trigger_play);
        } else {
            assert!(
                matches!(trigger_play, CardPlay::Cancelled),
                "declining skips discard and body"
            );
        }
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if accepted { 9_u8 } else { 0_u8 },
            "damage freezes the fee's effective cost before leaving hand"
        );
        assert_eq!(
            state["P1"]["hand"],
            if accepted {
                json!(["drawn"])
            } else {
                json!(["fuel"])
            },
            "draw follows only paid damage"
        );
    }
}

#[test]
fn bp16_058_all_printings_use_real_evolution_and_super_heads_then_check_end_phase_graveyard() {
    for (form, super_evolved, dragons, expected) in [
        ("BP16-058", false, 7_u8, 13_u8),
        ("BP16-SL16", true, 6_u8, 20_u8),
        ("PR-588", true, 7_u8, 13_u8),
    ] {
        let mut initial = position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-057"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..dragons)
                .map(|index| json!({"id":format!("grave-{index}"),"card":"read-dragon"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        let mut pay = json!({"pp":1_u8});
        if super_evolved {
            pay["sep"] = json!(1_u8);
        }
        let step = board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved", "the actual paired form evolves");
        for _ in 0_u8..if super_evolved { 2_u8 } else { 1_u8 } {
            resolved(pending(&mut board, true));
        }
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"], 9_u8,
            "evolve-time uses the paid card's old cost"
        );
        assert_eq!(
            state["P1"]["hand"],
            json!(["drawn"]),
            "the evolve head draws after discard"
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if super_evolved { 25_u8 } else { 20_u8 },
            "only super evolution adds five life"
        );
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        resolved(pending(&mut board, true));
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            expected,
            "end phase counts seven original expensive Dragons, excluding the neutral fee"
        );
    }
}

#[test]
fn bp04_116_all_printings_round_up_half_and_cannot_pay_without_an_enemy_target() {
    for (printing, cost, enemy, leader_life) in [
        ("BP04-116", 9_u8, true, 15_u8),
        ("BP04-SL21", 5_u8, true, 17_u8),
        ("BP04-116", 1_u8, true, 19_u8),
        ("BP04-SL21", 9_u8, false, 20_u8),
    ] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["hand"][0]["state"]["cost"] = json!(cost);
        if !enemy {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut board = game(&initial);
        let before = board.projection(View::Referee).unwrap();
        let play = board.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 });
        if !enemy {
            play.unwrap_err();
            assert_eq!(
                board.projection(View::Referee).unwrap(),
                before,
                "Q632 refuses payment without a follower target"
            );
            continue;
        }
        resolved(finish(&mut board, play.unwrap(), true));
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"], cost,
            "the selected follower receives the entire fee cost"
        );
        assert_eq!(
            state["P2"]["leader"]["life"], leader_life,
            "Q633 rounds odd costs upward for that follower's leader"
        );
        assert_eq!(
            state["objects"]["source"]["acted"], true,
            "the complete activation pays posture"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["fuel"]),
            "the fee really leaves hand"
        );
    }
}

#[test]
fn burned_knight_heads_cannot_discard_or_draw_without_an_enemy_follower() {
    for evolved in [false, true] {
        let mut initial = position();
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        if evolved {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-057"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP16-058"}]);
        } else {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .insert(0, json!({"id":"source","card":"BP16-057"}));
        }
        let mut board = game(&initial);
        if evolved {
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
                )
                .unwrap();
        } else {
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
        }
        assert!(
            matches!(pending(&mut board, true), CardPlay::Cancelled),
            "Q2109/Q2110 cancel a head without a legal follower target"
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            json!(["fuel"]),
            "no discard and no draw without the target"
        );
        assert_eq!(
            state["P1"]["deck"],
            json!(["drawn"]),
            "the replacement remains in deck"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            json!([]),
            "payment has not started"
        );
    }
}
