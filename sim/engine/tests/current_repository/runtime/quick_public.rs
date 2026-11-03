use super::*;
use sve_engine::game::internal::DecisionView;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|a| matches!(a["input"]["card_no"].as_str(),Some("BP04-079"|"ECP02-041"|"ECP02-SP08"|"BP04-T01"|"CP02-T01"|"PR-155"|"PR-156"|"BP03-T02")));
        for (number,traits) in [("quick-neutral",json!([])),("quick-triple",json!(["キュート","クール","パッション"])),("quick-cute-cool",json!(["キュート","クール"])),("quick-passion",json!(["パッション"]))] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"1","power":"1","hp":"20","text":null,"sections":[]}]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}

fn board(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "quick-public-real",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "quick-neutral",
        "quick-triple",
        "quick-cute-cool",
        "quick-passion",
    ] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

#[expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Quick protocol."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected Quick parameter");
    };
    question
}

#[expect(
    clippy::panic,
    reason = "Only the opponent's empty Quick turn may precede this actor."
)]
fn window(game: &mut Game) -> DecisionView {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let mut question = game.internal_quick_choice().unwrap().unwrap();
    if question.actor != (PlayerId { seat: 1 }) {
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        assert!(options.is_empty(), "the other player has no Quick source");
        drop(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
        );
        game.internal_rule_action(&json!({"do":"end-discard","select":[]})).unwrap();
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        question = game.internal_quick_choice().unwrap().unwrap();
    }
    assert_eq!(
        question.actor,
        PlayerId { seat: 1 },
        "the printed ability's owner chooses"
    );
    question
}

#[expect(
    clippy::panic,
    reason = "Only the source's printed Quick and formal target are expected."
)]
fn select_source(game: &mut Game, question: &DecisionView) -> DecisionView {
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick");
    };
    assert_eq!(
        options.len(),
        1,
        "only this complete printed ability is payable"
    );
    let target = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap(),
    );
    assert!(
        matches!(target.domain, InputDomain::Targets(_)),
        "formal targets precede material payment"
    );
    awaiting(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    )
}

#[test]
fn medusa_all_printings_create_once_then_quote_and_pay_two_own_vipers() {
    for printing in ["BP04-079", "BP04-SL15", "BP04-U05"] {
        for existing in [0_u8, 1_u8] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P2"]["zones"]["deck"] = json!([
                {"id":"opponent-draw","card":"quick-neutral"}
            ]);
            let mut field = vec![json!({"id":"source","card":printing,"state":{"acted":false}})];
            if existing == 1 {
                field.push(json!({"id":"old-viper","card":"BP04-T01"}));
            }
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"quick-neutral"},{"id":"foreign-viper0","card":"BP04-T01"},{"id":"foreign-viper1","card":"BP04-T01"}]);
            let mut game = board(&initial);
            let creator = prepared().current[printing]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| a.node.play.as_ref().is_some_and(|p| p.limit.is_some()))
                .unwrap()
                .node
                .id;
            resolved(
                game.activate_internal_card("source", creator, PlayerId { seat: 1 })
                    .unwrap(),
            );
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            game.activate_internal_card("source", creator, PlayerId { seat: 1 })
                .unwrap_err();
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["P1"]["field"].as_array().unwrap().len(),
                usize::from(existing) + 2,
                "the zero-PP head creates exactly one Viper"
            );
            assert_eq!(
                before["P1"]["pp"]["current"], 10_u8,
                "creation costs zero PP"
            );
            let question = window(&mut game);
            let InputDomain::Quick { options, .. } = &question.domain else {
                panic!("Quick");
            };
            assert_eq!(
                options.len(),
                usize::from(existing == 1),
                "foreign Vipers cannot fill the fee"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                false,
                "quoting cannot Act the source"
            );
            if existing == 1 {
                let payment = select_source(&mut game, &question);
                let InputDomain::CostMaterials(materials) = &payment.domain else {
                    panic!("two Vipers");
                };
                assert_eq!(
                    materials.members.len(),
                    2,
                    "only the two own Vipers qualify"
                );
                resolved(
                    game.answer_internal_card(
                        payment.actor,
                        payment.token,
                        Answer::CostMaterials(vec![0, 1]),
                    )
                    .unwrap(),
                );
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(
                    after["P1"]["field"],
                    json!(["source"]),
                    "only Vipers leave; Q610 does not sacrifice Medusa"
                );
                assert_eq!(after["objects"]["source"]["acted"], true, "Medusa pays Act");
                assert!(
                    after["P2"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("enemy")),
                    "the formal enemy is destroyed"
                );
                assert_eq!(
                    after["P2"]["field"],
                    json!(["foreign-viper0", "foreign-viper1"]),
                    "opponent materials remain untouched"
                );
            }
        }
    }
}

#[test]
fn riamu_both_card_ids_and_all_printings_pay_hand_quick_pp_lesson_and_source_discard() {
    for printing in ["ECP02-041", "ECP02-SL21", "ECP02-SP08", "ECP02-U08"] {
        for (pp, item, enemy) in [
            (0_u8, true, true),
            (1, false, true),
            (1, true, false),
            (1, true, true),
        ] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!("P2");
            initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["ex"] = if item {
                json!([{"id":"lesson","card":"PR-155"},{"id":"wrong","card":"quick-neutral"}])
            } else {
                json!([{"id":"wrong","card":"quick-neutral"}])
            };
            initial["players"]["P2"]["zones"]["ex"] =
                json!([{"id":"foreign-item","card":"PR-155"}]);
            initial["players"]["P2"]["zones"]["field"] = if enemy {
                json!([{"id":"enemy","card":"quick-neutral"}])
            } else {
                json!([])
            };
            let mut game = board(&initial);
            let question = window(&mut game);
            let before = game.projection(View::Referee).unwrap();
            let InputDomain::Quick { options, .. } = &question.domain else {
                panic!("Quick");
            };
            assert_eq!(
                options.len(),
                usize::from(pp == 1 && item && enemy),
                "quote proves PP, a real own EX Lesson item, and a formal enemy"
            );
            assert_eq!(
                before["P1"]["hand"],
                json!(["source"]),
                "quoting never discards the source"
            );
            if pp == 1 && item && enemy {
                let payment = select_source(&mut game, &question);
                let InputDomain::CostMaterials(materials) = &payment.domain else {
                    panic!("Lesson");
                };
                assert_eq!(
                    materials.members.len(),
                    1,
                    "wrong names and opponent EX cannot pay Lesson"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    1_u8,
                    "selection precedes every payment"
                );
                resolved(
                    game.answer_internal_card(
                        payment.actor,
                        payment.token,
                        Answer::CostMaterials(vec![0]),
                    )
                    .unwrap(),
                );
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(
                    after["P1"]["pp"]["current"], 0_u8,
                    "the activation pays one PP, not the printed eight"
                );
                assert_eq!(
                    after["P1"]["ex"],
                    json!(["wrong"]),
                    "exactly one real Lesson item is banished"
                );
                assert_eq!(
                    after["P1"]["hand"],
                    json!([]),
                    "the source is discarded from hand"
                );
                assert!(
                    after["P1"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("source")),
                    "source discard completes before damage"
                );
                assert_eq!(
                    after["objects"]["enemy"]["damage"], 2_u8,
                    "the complete Quick body deals two"
                );
            }
        }
    }
}

#[test]
fn riamu_both_complete_fanfares_require_all_three_grave_types_for_storm_and_pp() {
    for printing in ["ECP02-041", "ECP02-SL21", "ECP02-SP08", "ECP02-U08"] {
        for sufficient in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            let mut grave=(0_u8..3).map(|i|json!({"id":format!("grave{i}"),"card":if sufficient {"quick-triple"} else {"quick-cute-cool"}})).collect::<Vec<_>>();
            if !sufficient {
                grave.extend(
                    (0_u8..2).map(|i| json!({"id":format!("passion{i}"),"card":"quick-passion"})),
                );
            }
            initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy0","card":"quick-neutral"},{"id":"enemy1","card":"quick-neutral"}]);
            let mut game = board(&initial);
            assert!(
                matches!(
                    game.play_internal_card("source", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "printed eight-PP follower enters"
            );
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("Fanfare");
            };
            resolved(
                game.answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            for id in ["enemy0", "enemy1"] {
                assert_eq!(
                    after["objects"][id]["damage"], 7_u8,
                    "the seven damage is unconditional"
                );
            }
            assert_eq!(
                after["P1"]["pp"]["current"],
                if sufficient { 3_u8 } else { 2_u8 },
                "English source puts PP recovery under the same three-type condition"
            );
            assert_eq!(
                after["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                sufficient,
                "Q2171 allows three multi-trait cards to satisfy all types"
            );
        }
    }
}
