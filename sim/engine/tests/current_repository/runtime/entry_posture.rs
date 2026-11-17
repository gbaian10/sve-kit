use super::event_cemetery::finish_checks;
use super::event_subject_capture::prepared_program;
use super::union_burst_batch1::finish;
use super::*;
use sve_engine::game::internal::SearchAnswer;

const CARDS: [(&str, &str); 2] = [("BP01-150", "死の宣告"), ("SD06-016", "聖獣への誓い")];

#[test]
fn printed_amulets_enter_acted_from_hand_and_ex_without_a_posture_event() {
    for (number, _) in CARDS {
        for zone in ["hand", "ex"] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
            let mut game = game(&initial);
            let CardPlay::Entered(entered) = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("printed amulet entry")
            };
            assert!(
                entered
                    .progress
                    .facts
                    .iter()
                    .all(|fact| !matches!(fact, Fact::Posture(_)))
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                true
            );
            finish_checks(&mut game);
            let attempt =
                game.activate_internal_card("source", activation(number), PlayerId { seat: 1 });
            let refusal = match attempt {
                Ok(CardPlay::Awaiting(question)) => {
                    let InputDomain::Targets(domain) = &question.domain else {
                        panic!("formal activation target")
                    };
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(vec![domain.members[0].handle]),
                    )
                }
                remaining => remaining,
            };
            assert!(matches!(refusal, Err(_) | Ok(CardPlay::Cancelled)));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["field"],
                json!(["source"])
            );
        }
    }
}

#[test]
fn chosen_public_zone_cards_share_the_intrinsic_entry_posture_rule() {
    for (number, _) in CARDS {
        for zone in ["ex", "cemetery", "banish"] {
            let mut tree = expanded::tree().clone();
            let mut card = tree["cards"]["BP01-041"].clone();
            let mut ability = card["abilities"][0].clone();
            ability["activate"] = json!({"cost":{"pp":1_u8},"targets":[{"bind":"chosen","from":{"who":"self","zone":zone,"kind":"amulet"},"n":1_u8}],"do":[{"move":{"ref":"chosen","to":"field","result":"entered"}}]});
            card["abilities"] = json!([ability]);
            tree["cards"] = json!({"producer":card});
            let program = lowerer().lower(&tree).unwrap().remove("producer").unwrap();
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"][zone] =
                json!([{"id":"source","card":number,"state":{"face_up":true}}]);
            let mut game = prepared_program(&initial, &program);
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("public movement target")
            };
            let InputDomain::Targets(domain) = question.domain else {
                panic!("public movement target domain")
            };
            assert_eq!(domain.members.len(), 1);
            let play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            let entered = resolved(finish(&mut game, play));
            assert!(
                entered
                    .progress
                    .facts
                    .iter()
                    .all(|fact| !matches!(fact, Fact::Posture(_)))
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                true
            );
        }
    }
}

#[test]
fn a_standing_death_declaration_pays_itself_before_destroying_its_actual_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-150"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    let declaration = game
        .activate_internal_card("source", activation("BP01-150"), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, declaration));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn paying_the_tiger_amulet_releases_a_full_fields_slot_before_its_token_enters() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"SD06-016"},
        {"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},
        {"id":"c","card":"BP01-171"},{"id":"d","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    let declaration = game
        .activate_internal_card("source", activation("SD06-016"), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, declaration));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    let token = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| !["a", "b", "c", "d"].contains(id))
        .unwrap();
    assert_eq!(state["objects"][token]["power"], 4_u8);
    assert_eq!(state["objects"][token]["hp"], 4_u8);
    assert!(
        state["objects"][token]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}

#[test]
fn an_entry_posture_rule_requires_the_field_and_its_prospective_characteristics() {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["static"] = json!({"effects":[{"enters_acted":{"on":"this"}}]});
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"entry-subject":card});
    let valid = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("entry-subject")
        .unwrap();
    for prospective in [false, true] {
        let mut changed = valid.card().clone();
        let definition = &mut changed.abilities[0].node;
        if prospective {
            let Effect::Replacement {
                prospective_chars, ..
            } = &mut definition.declarations[0]
            else {
                panic!("entry replacement")
            };
            *prospective_chars = None;
        } else {
            definition.active_zones = vec![ZoneKind::Hand];
        }
        WellFormedCardProgram::check(changed).unwrap_err();
    }
}

#[test]
fn rodeos_real_evolution_search_puts_either_printed_amulet_directly_into_play_acted() {
    for (number, _) in CARDS {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"rodeo","card":"BP16-094"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-095"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"amulet","card":number}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"rodeo","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_checks(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = pending.domain else {
            panic!("Rodeo evolution trigger")
        };
        let CardPlay::Awaiting(search) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("Rodeo deck search")
        };
        let InputDomain::Search(domain) = search.domain else {
            panic!("printed amulet search")
        };
        assert_eq!(domain.cards.len(), 1);
        let play = game
            .answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap();
        let result = resolved(finish(&mut game, play));
        assert!(
            result
                .progress
                .facts
                .iter()
                .all(|fact| !matches!(fact, Fact::Posture(_)))
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["rodeo", "amulet"]));
        assert_eq!(state["objects"]["amulet"]["acted"], true);
    }
}

#[test]
fn kaguyas_real_private_hand_choice_puts_either_amulet_into_play_acted() {
    for (number, _) in CARDS {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"kaguya","card":"BP02-092"},{"id":"amulet","card":number}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("kaguya", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = pending.domain else {
            panic!("Kaguya Fanfare")
        };
        let play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let entered = resolved(finish(&mut game, play));
        assert!(
            entered
                .progress
                .facts
                .iter()
                .all(|fact| !matches!(fact, Fact::Posture(_)))
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["kaguya", "amulet"]));
        assert_eq!(state["objects"]["amulet"]["acted"], true);
    }
}
