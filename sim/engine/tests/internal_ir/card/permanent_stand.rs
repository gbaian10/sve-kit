use super::*;

fn tree(on: &str, who: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut definition = tree["cards"]["BP08-038"].clone();
    let mut header = definition["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("fanfare");
    header["static"] = json!({"effects":[{"cannot":{"what":"stand","on":on,"during":{"phase":"start","who":who,"cause":"rule"}}}]});
    definition["abilities"] = json!([header]);
    tree["cards"] = json!({"unit-follower":definition});
    tree
}

fn program() -> WellFormedCardProgram {
    let lowered = lowerer().lower(&tree("this", "self")).unwrap();
    let mut restriction = lowered["unit-follower"].card().abilities[0].clone();
    restriction.source = source("unit-follower");
    restriction.source.fragment = None;
    restriction.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    card(vec![restriction])
}

fn position(seat: u32, silenced: bool) -> Game {
    let mut initial = setup(seat, json!([]));
    let actor = if seat == 1 { "P1" } else { "P2" };
    let other = if seat == 1 { "P2" } else { "P1" };
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":true,"silenced":silenced}}]);
    initial["players"][other]["zones"]["field"] = json!([]);
    for player in [actor, other] {
        initial["players"][player]["zones"]["hand"] = json!([]);
        initial["players"][player]["zones"]["deck"] = json!(
            (0..8)
                .map(|i| json!({"id":format!("{player}-d{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "permanent-stand",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program())
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn end(game: &mut Game) {
    if game.internal_card_stage().is_some() {
        game.resume_internal_card().unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn permanent_stand_restriction_survives_multiple_own_starts_and_allows_effect_stand() {
    for seat in [1, 2] {
        let mut game = position(seat, false);
        for _ in 0..3 {
            end(&mut game);
            end(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
                true
            );
        }
        let action = instruction(
            1,
            Action::Posture {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                posture: Posture::Standing,
            },
        );
        game.execute_internal(
            &checked(vec![action]),
            &source("unit-follower"),
            context(seat),
        )
        .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
            false
        );
    }
}

#[test]
fn losing_the_printed_ability_restores_rule_start_stand() {
    let mut game = position(1, true);
    end(&mut game);
    end(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
        false
    );
}

#[test]
fn permanent_stand_restriction_rejects_other_players_affected_and_extra_conditions() {
    for (on, who) in [("affected", "self"), ("this", "opp")] {
        lowerer().lower(&tree(on, who)).unwrap_err();
    }
    let mut conditional = tree("this", "self");
    conditional["cards"]["unit-follower"]["abilities"][0]["static"]["while"] =
        json!({"awakened":true});
    lowerer().lower(&conditional).unwrap_err();
}
