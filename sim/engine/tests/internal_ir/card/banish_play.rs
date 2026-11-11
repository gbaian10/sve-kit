use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::InstalledChange;
use sve_engine::ir::lowering::LoweringError;

fn permission() -> Value {
    json!({"player_effect":{"who":"self","until":"turn","effect":{"play_permission":{"from":["banish"],"on":{"who":"self","zone":"banish"}}}}})
}
fn grant() -> Value {
    json!({"cost":{"pp":1_u8},"do":[permission()]})
}
fn tree(installation: Value, restriction: Option<Value>) -> Value {
    let mut tree = evolution_linked_movement::tree(&[("activate", installation)]);
    let grant = tree["cards"]["unit-follower"].take();
    let heal = evolution_linked_movement::tree(&[(
        "spell",
        json!({"do":[{"life":{"who":"self","delta":3_u8}}]}),
    )]);
    let mut spell = heal["cards"]["unit-follower"].clone();
    if let Some(condition) = restriction {
        let mut header = spell["abilities"][0].clone();
        header.as_object_mut().unwrap().remove("spell");
        header["line"] = json!(2_u8);
        header["play"] = json!({"cannot_if":condition});
        spell["abilities"].as_array_mut().unwrap().push(header);
    }
    let later = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0_u8},"do":[{"banish":"this"}]}),
    )]);
    tree["cards"] = json!({"unit-amulet":grant,"unit-spell":spell,"unit-follower":later["cards"]["unit-follower"]});
    tree
}
fn lower(tree: &Value) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
    lowerer().lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
}
fn programs() -> BTreeMap<String, WellFormedCardProgram> {
    lower(&tree(grant(), None)).unwrap()
}
fn initial(pp: u8) -> Value {
    let mut initial = setup(
        1,
        json!([{"id":"a0","card":"unit-spell"},{"id":"a1","card":"unit-spell"}]),
    );
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"grant","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"banished","card":"unit-spell"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"b0","card":"unit-spell"},{"id":"b1","card":"unit-spell"}]);
    initial["players"]["P2"]["zones"]["banish"] = json!([{"id":"foreign","card":"unit-spell"}]);
    initial
}
fn position(programs: &BTreeMap<String, WellFormedCardProgram>, initial: &Value) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "banish-play",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn install(game: &mut Game, programs: &BTreeMap<String, WellFormedCardProgram>) {
    resolved(
        game.activate_internal_card(
            "grant",
            programs["unit-amulet"].card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    finish(game);
}
#[test]
fn banish_play_permission_keeps_normal_payment_owner_and_resolution_cleanup() {
    let mut installation = grant();
    installation["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"destroy":"this"}));
    let programs = lower(&tree(installation, None)).unwrap();
    let mut game = position(&programs, &initial(2));
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("banished", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    install(&mut game, &programs);
    let observation = game.observe(Viewer::P1).unwrap();
    let player = observation
        .players
        .iter()
        .find(|player| player.player == Viewer::P1)
        .unwrap();
    let permission = player
        .effects
        .iter()
        .find(|effect| effect.change == InstalledChange::BanishPlay)
        .unwrap();
    assert!(permission.source.is_some());
    assert!(
        permission.source.as_ref().unwrap().holder.is_none(),
        "a departed provider has no current link"
    );
    for (holder, seat) in [("foreign", 1), ("banished", 2)] {
        let before = game.projection(View::Referee).unwrap();
        game.play_internal_card(holder, PlayerId { seat })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
    resolved(
        game.play_internal_card("banished", PlayerId { seat: 1 })
            .unwrap(),
    );
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 0);
    assert_eq!(after["P1"]["leader"]["life"], 23);
    assert_eq!(after["P1"]["banish"], json!([]));
    assert_eq!(after["P1"]["cemetery"], json!(["grant", "banished"]));
    assert_eq!(after["P1"]["field"], json!([]));
    assert_eq!(after["P2"]["banish"], json!(["foreign"]));
}
#[test]
fn banish_play_permission_also_plays_an_amulet_with_its_normal_field_entry() {
    let programs = programs();
    let mut start = initial(3);
    start["players"]["P1"]["zones"]["banish"] = json!([{"id":"recovered","card":"unit-amulet"}]);
    let mut game = position(&programs, &start);
    install(&mut game, &programs);
    assert!(matches!(
        game.play_internal_card("recovered", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["grant", "recovered"]));
    assert_eq!(after["P1"]["banish"], json!([]));
    assert_eq!(after["P1"]["pp"]["current"], 1);
}
#[test]
fn banish_play_permission_does_not_waive_the_printed_cost() {
    let programs = programs();
    let mut game = position(&programs, &initial(1));
    install(&mut game, &programs);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("banished", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
#[test]
fn banish_play_permission_applies_to_later_arrivals_and_stops_at_turn_end() {
    let programs = programs();
    let mut start = initial(4);
    start["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"later","card":"unit-follower"}));
    let mut game = position(&programs, &start);
    install(&mut game, &programs);
    resolved(
        game.activate_internal_card(
            "later",
            programs["unit-follower"].card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    finish(&mut game);
    assert!(matches!(
        game.play_internal_card("later", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["grant", "later"])
    );
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        finish(&mut game);
        quick::pass(&mut game);
        finish(&mut game);
    }
    resolved(game.play_internal_card("a0", PlayerId { seat: 1 }).unwrap());
    finish(&mut game);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("banished", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
#[test]
fn banish_play_permission_keeps_intrinsic_prohibitions_but_respects_ex_only_scope() {
    for (condition, forbidden) in [
        (json!({"my_turn":true}), true),
        (json!({"in_zone":{"ref":"this","zone":"ex"}}), false),
    ] {
        let programs = lower(&tree(grant(), Some(condition))).unwrap();
        let mut game = position(&programs, &initial(2));
        install(&mut game, &programs);
        let result = game.play_internal_card("banished", PlayerId { seat: 1 });
        if forbidden {
            result.unwrap_err();
        } else {
            resolved(result.unwrap());
            finish(&mut game);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if forbidden { 20 } else { 23 }
        );
    }
}
#[test]
fn banish_play_permission_composes_with_next_spell_discount_without_granting_permission_itself() {
    for with_permission in [false, true] {
        let mut installation = grant();
        installation["do"] = if with_permission {
            json!([permission()])
        } else {
            json!([])
        };
        installation["do"].as_array_mut().unwrap().push(json!({"player_effect":{"who":"self","until":"turn","effect":{"cost_mod":{"when_playing":{"who":"self","kind":"spell"},"delta":-1,"next_only":true}}}}));
        let programs = lower(&tree(installation, None)).unwrap();
        let mut start = initial(1);
        start["players"]["P1"]["zones"]["banish"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"unit-spell"}));
        let mut game = position(&programs, &start);
        install(&mut game, &programs);
        let result = game.play_internal_card("banished", PlayerId { seat: 1 });
        if with_permission {
            resolved(result.unwrap());
            finish(&mut game);
        } else {
            result.unwrap_err();
        }
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["pp"]["current"], 0);
        assert_eq!(
            before["P1"]["leader"]["life"],
            if with_permission { 23 } else { 20 }
        );
        game.play_internal_card("second", PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}
#[test]
fn banish_play_permission_opens_only_legal_quick_spells_in_the_owners_question() {
    let mut installation = grant();
    installation["quick"] = json!(true);
    let mut programs = lower(&tree(installation, None)).unwrap();
    let spell = programs["unit-spell"].card().abilities[0].clone();
    programs.insert("unit-spell".into(), card(vec![spell, quick::permit()]));
    let mut start = initial(3);
    start["turn"]["active"] = json!("P2");
    let mut game = position(&programs, &start);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish(&mut game);
    let first = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &first.domain else {
        panic!("Quick window")
    };
    assert!(options.iter().all(|option| option.zone != "banish"));
    let handle = options
        .iter()
        .find(|option| option.line.is_some())
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(first.actor, first.token, Answer::Quick(Some(handle)))
            .unwrap(),
    );
    finish(&mut game);
    let second = game.internal_quick_choice().unwrap().unwrap();
    assert!(game.internal_decision(View::P2).is_none());
    let InputDomain::Quick { options, .. } = &second.domain else {
        panic!("Quick window")
    };
    let handle = options
        .iter()
        .find(|option| option.zone == "banish")
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(second.actor, second.token, Answer::Quick(Some(handle)))
            .unwrap(),
    );
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
}
#[test]
fn banish_play_permission_rejects_other_players_zones_lifetimes_and_object_installation() {
    for patch in [
        json!({"who":"opp"}),
        json!({"until":"game"}),
        json!({"effect":{"play_permission":{"from":["cemetery"],"on":{"who":"self","zone":"cemetery"}}}}),
    ] {
        let mut step = permission();
        for (key, value) in patch.as_object().unwrap() {
            step["player_effect"][key] = value.clone();
        }
        lower(&tree(json!({"cost":{"pp":0},"do":[step]}), None)).unwrap_err();
    }
    let applied = json!({"apply":{"on":"this","until":"turn","effect":{"play_permission":{"from":["banish"],"on":{"who":"self","zone":"banish"}}}}});
    lower(&tree(json!({"cost":{"pp":0},"do":[applied]}), None)).unwrap_err();
}
#[test]
fn banish_play_permission_raw_ir_cannot_install_an_unbounded_lifetime() {
    let mut raw = programs()["unit-amulet"].card().clone();
    let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("installation")
    };
    let Action::InstallEffect {
        effect, duration, ..
    } = &mut instruction.action
    else {
        panic!("installation")
    };
    duration.end = DurationEnd::NoTemporalExpiry {};
    let Effect::Permission {
        duration: inner, ..
    } = effect.as_mut()
    else {
        panic!("permission")
    };
    *inner = duration.clone();
    WellFormedCardProgram::check(raw).unwrap_err();
}
