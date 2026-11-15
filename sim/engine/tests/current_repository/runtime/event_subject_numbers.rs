#![expect(
    clippy::panic,
    reason = "Unexpected decisions or shapes invalidate the numeric event scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

#[test]
fn fangslayer_uses_each_departed_followers_modified_power() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-025"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"execution","card":"BP01-169"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"power":7_u8}}]);
    let mut game = game(&initial);
    let declaration = game
        .play_internal_card("execution", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, declaration));
    finish_checks(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("unexpected numeric event fixture shape")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP22-025");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(finish(&mut game, play));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        13_u8
    );
}

fn resolve_number(game: &mut Game, number: &str, line: u64) {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("unexpected numeric event fixture shape")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    resolved(finish(game, play));
    finish_checks(game);
}

#[test]
fn kaguya_puts_a_hand_amulet_into_play_and_reads_its_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP02-092"}, {"id":"amulet","card":"SD06-015"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolve_number(&mut game, "BP02-092", 3);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source", "amulet"])
    );
    resolve_number(&mut game, "BP02-092", 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        9_u8
    );
}

#[test]
fn evolved_kaguya_creates_its_real_moon_heals_and_triggers_for_that_amulet() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-092"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-093"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    resolve_number(&mut game, "BP02-093", 2);
    resolve_number(&mut game, "BP02-093", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 13_u8);
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(state["objects"]["enemy"]["hp"], 7_u8);
}

#[test]
fn each_entry_retains_its_own_cost_after_the_amulets_leave() {
    use super::event_cemetery::move_named;
    use super::event_subject_capture::{enter, prepared};
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-092"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"first","card":"SD06-015"}, {"id":"second","card":"SD06-015"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut game = prepared(
        &initial,
        vec![
            enter(ZoneKind::Ex, "白翼への祈り"),
            move_named(ZoneKind::Field, OwnerZone::Cemetery, "白翼への祈り"),
        ],
    );
    activate(&mut game, "P1");
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["first", "second"])
    );
    resolve_number(&mut game, "BP02-092", 2);
    resolve_number(&mut game, "BP02-092", 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        8_u8
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn simultaneous_departures_keep_independent_powers_including_erased_tokens() {
    use super::event_cemetery::move_named;
    use super::event_subject_capture::prepared;
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-025"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-T04","state":{"power":3_u8}},
        {"id":"second","card":"BP01-T04","state":{"power":7_u8}}
    ]);
    let action = move_named(ZoneKind::Field, OwnerZone::Cemetery, "乙姫お守り隊");
    let mut game = prepared(&initial, vec![action]);
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_number(&mut game, "BP22-025", 2);
    resolve_number(&mut game, "BP22-025", 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        10_u8
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn dragon_canyon_plays_its_top_deck_follower_and_uses_that_entrys_power() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-073"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"dragon","card":"BP01-092"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolve_number(&mut game, "BP22-073", 2);
    resolve_number(&mut game, "BP22-073", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "dragon"]));
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(state["objects"]["enemy"]["hp"], 9_u8);
}

#[test]
fn dragon_canyon_keeps_the_entry_power_when_the_follower_leaves_before_resolution() {
    use super::event_cemetery::move_named;
    use super::event_subject_capture::{enter, prepared};
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-073"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"dragon","card":"BP01-080"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
    let mut game = prepared(
        &initial,
        vec![
            enter(ZoneKind::Ex, "ジェネシスドラゴン"),
            move_named(ZoneKind::Field, OwnerZone::Cemetery, "ジェネシスドラゴン"),
        ],
    );
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_number(&mut game, "BP22-073", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["dragon"]));
    assert_eq!(state["objects"]["enemy"]["hp"], 10_u8);
}

#[test]
fn numeric_event_views_require_their_exact_subject_binding_and_snapshot_policy() {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["auto"] = json!({"on":{"enter_field":{"who":"self","sel":{"kind":"follower"}}},"do":[{"damage":{"to":{"leaders":"opponents"},"n":{"power_of":"it"}}}]});
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"numeric-subject":card});
    for property in ["orig_cost_of", "cost_of", "power_of"] {
        tree["cards"]["numeric-subject"]["abilities"][0]["auto"]["do"][0]["damage"]["n"] =
            json!({property:"it"});
        lowerer().lower(&tree).unwrap();
    }
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("numeric-subject")
        .unwrap();
    for change in 0_u8..3 {
        let mut changed = program.card().clone();
        let body = changed.abilities[0].node.body.as_mut().unwrap();
        let scope = body.scope;
        let Node::Instruction { instruction } = &mut body.steps[0].node else {
            panic!("unexpected numeric event fixture shape")
        };
        let Action::Damage {
            amount: DamageAmount::Uniform { amount },
            ..
        } = &mut instruction.action
        else {
            panic!("unexpected numeric event fixture shape")
        };
        let Expr::Compute(operation) = amount else {
            panic!("unexpected numeric event fixture shape")
        };
        let IntOperation::InformationOrZero {
            information: NumberRead::Stats { object, .. },
        } = operation.as_mut()
        else {
            panic!("unexpected numeric event fixture shape")
        };
        let ObjectView::EventSnapshot {
            object,
            event,
            policy,
        } = object
        else {
            panic!("unexpected numeric event fixture shape")
        };
        match change {
            0 => **object = ObjectOperand::SourceHolder {},
            1 => *policy = EventSnapshotPolicy::BeforeExit,
            _ => {
                let EventContext::EnterField { event: binding } = event else {
                    panic!("unexpected numeric event fixture shape")
                };
                *binding = Binding::new(999, scope);
            }
        }
        WellFormedCardProgram::check(changed).unwrap_err();
    }
    tree["cards"]["numeric-subject"]["abilities"][0]
        .as_object_mut()
        .unwrap()
        .remove("auto");
    tree["cards"]["numeric-subject"]["abilities"][0]["activate"] = json!({"cost":{"pp":1_u8},"do":[{"damage":{"to":{"leaders":"opponents"},"n":{"power_of":"it"}}}]});
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn kaguya_may_leave_a_matching_amulet_in_its_private_hand() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP02-092"},{"id":"amulet","card":"SD06-015"}
    ]);
    let mut game = game(&initial);
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish_checks(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("unexpected numeric event fixture shape")
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(choice) = play else {
        panic!("unexpected numeric event fixture shape")
    };
    assert!(matches!(choice.domain, InputDomain::Targets(_)));
    resolved(
        game.answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["amulet"]));
    assert_eq!(state["P1"]["field"], json!(["source"]));
}
