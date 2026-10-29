use super::*;
use sve_engine::game::internal::InputDomain;

fn candidate(zone: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut definition = tree["cards"]["BP08-038"].clone();
    let mut ability = definition["abilities"][0].clone();
    ability["fanfare"] = json!({"do":[{"if":{
        "cond":{"played_from":{"ref":"this","from":zone}},
        "then":[{"draw":{"n":1}}]
    }}]});
    definition["abilities"] = json!([ability]);
    tree["cards"] = json!({"unit-follower":definition});
    tree
}

#[test]
fn played_from_accepts_the_registered_ex_origin() {
    lowerer().lower(&candidate("ex")).unwrap();
}

#[test]
fn played_from_rejects_other_origins_and_nonentry_abilities() {
    for zone in ["hand", "field", "cemetery", "deck"] {
        lowerer().lower(&candidate(zone)).unwrap_err();
    }
    let mut tree = candidate("ex");
    let ability = &mut tree["cards"]["unit-follower"]["abilities"][0];
    let body = ability.as_object_mut().unwrap().remove("fanfare").unwrap();
    ability["activate"] = body;
    ability["activate"]["cost"] = json!({"pp":0});
    let mut supported = tree.clone();
    supported["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] =
        json!([{"draw":{"n":1}}]);
    lowerer().lower(&supported).unwrap();
    lowerer().lower(&tree).unwrap_err();
}

fn program(zone: &str, origin: ZoneKind) -> WellFormedCardProgram {
    let cards = lowerer().lower(&candidate(zone)).unwrap();
    let mut trigger = cards["unit-follower"].card().abilities[0].clone();
    trigger.source = source("unit-follower");
    trigger.source.fragment = None;
    if let AbilityOrigin::Keyword { origin, .. } = &mut trigger.node.source.origin {
        **origin = AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        };
    }
    locate_steps(
        &mut trigger.node.body.as_mut().unwrap().steps,
        &trigger.source,
        &trigger.node.source,
    );
    card(vec![
        trigger,
        ability(9, vec![entry::enter(90, entry::own_zone(origin, 9))]),
    ])
}

#[test]
fn played_from_reads_the_frozen_origin_and_requires_an_actual_card_play() {
    let wanted = "ex";
    for (from, played) in [
        ("hand", true),
        ("ex", true),
        ("ex", false),
        ("cemetery", false),
    ] {
        let zone = if from == "cemetery" {
            ZoneKind::Cemetery
        } else {
            ZoneKind::Ex
        };
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"][from] = json!([{"id":"incoming","card":"unit-follower","state":{"entered_from":if from == "ex" { "hand" } else { "ex" },"entered_by":"play","entered_by_play":true}}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "play-origin",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program(wanted, zone))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        if played {
            assert!(
                matches!(
                    game.play_internal_card("incoming", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "the printed card enters by a proper play"
            );
        } else {
            resolved(
                game.activate_internal_card("s", AbilityId(9), PlayerId { seat: 1 })
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        let question = game.internal_pending_choice().unwrap().unwrap();
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if played && from == wanted {
                json!(["drawn"])
            } else {
                json!([])
            },
            "matched entry authority overrides spoofed persistent flags"
        );
    }
}

#[test]
fn played_from_keeps_the_original_ex_play_after_its_source_leaves() {
    let mut tree = candidate("ex");
    let mut departure = tree["cards"]["unit-follower"]["abilities"][0].clone();
    departure["line"] = json!(2);
    departure["fanfare"] = json!({"do":[{"move":{"ref":"this","to":"cemetery"}}]});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(departure);
    let cards = lowerer().lower(&tree).unwrap();
    let mut abilities = cards["unit-follower"].card().abilities.clone();
    for (ability, line) in abilities.iter_mut().zip(1_u64..) {
        ability.source = source("unit-follower");
        ability.source.fragment = None;
        ability.source.line = u32::try_from(line).unwrap();
        if let AbilityOrigin::Keyword { origin, .. } = &mut ability.node.source.origin {
            **origin = AbilityOrigin::Printed {
                face: FaceId("0".into()),
                line: Nat(line),
            };
        }
        locate_steps(
            &mut ability.node.body.as_mut().unwrap().steps,
            &ability.source,
            &ability.node.source,
        );
    }
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"incoming","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"unit-spell"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "frozen-play-origin",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &card(abilities))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    assert!(
        matches!(
            game.play_internal_card("incoming", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the source is played from EX"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("two entry triggers");
    };
    let handle = options
        .iter()
        .find(|option| option.line == 2)
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["incoming"]),
        "the source has left the field"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["drawn"]),
        "its original EX play remains the deciding fact"
    );
}
