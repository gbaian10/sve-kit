use super::*;
use sve_engine::game::internal::InputDomain;

fn body() -> Value {
    json!({
        "targets":[{"bind":"enemy","option":1,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"if":{"cond":{"awakened":true},"then":[{"choice":{"n":1,"options":[
            [{"damage":{"to":"enemy","n":1}}],
            [{"damage":{"to":{"who":"opp","zone":"field","kind":"follower"},"n":2}}]
        ]}}]}}]
    })
}

fn tree(head: &str, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry[head] = body;
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn position(typed: &WellFormedCardProgram, maximum: u8, available: bool) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = if available {
        json!([{"id":"enemy","card":"unit-follower"}])
    } else {
        json!([])
    };
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "conditional-modes",
    )
    .unwrap();
    game.load_internal_card("unit-follower", typed).unwrap();
    game
}

#[test]
fn fanfare_choice_lowers_both_unconditional_and_awakened_modes() {
    lowerer().lower(&tree("fanfare", body())).unwrap();
    let mut direct = body();
    direct["do"] = direct["do"][0]["if"]["then"].clone();
    lowerer().lower(&tree("fanfare", direct.clone())).unwrap();
    direct.as_object_mut().unwrap().remove("targets");
    direct["do"][0]["choice"]["options"][0] = json!([{"life":{"who":"self","delta":1}}]);
    lowerer().lower(&tree("fanfare", direct)).unwrap();
}

#[test]
fn conditional_choice_rejects_other_heads_conditions_and_else_branches() {
    lowerer().lower(&tree("fanfare", body())).unwrap();
    lowerer().lower(&tree("attack_time", body())).err().unwrap();
    let mut other = body();
    other["do"][0]["if"]["cond"] = json!({"leader_life":{"who":"self","le":10}});
    lowerer().lower(&tree("fanfare", other)).err().unwrap();
    let mut alternative = body();
    alternative["do"][0]["if"]["else"] = json!([{"life":{"who":"self","delta":1}}]);
    lowerer()
        .lower(&tree("fanfare", alternative))
        .err()
        .unwrap();
    let mut extra = body();
    extra["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"draw":{"n":1}}));
    lowerer().lower(&tree("fanfare", extra)).err().unwrap();
}

#[test]
fn raw_modes_cannot_bypass_the_play_time_condition_contract() {
    let typed = lowerer()
        .lower(&tree("fanfare", body()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut candidate = typed.card().clone();
    let Node::Control {
        control: Control::If { condition, .. },
    } = &mut candidate.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("guarded mode")
    };
    *condition = Expr::Literal(Bool(true));
    WellFormedCardProgram::check(candidate).err().unwrap();
}

#[test]
fn inactive_fanfare_skips_modes_and_active_second_mode_skips_first_targets() {
    let guarded = lowerer()
        .lower(&tree("fanfare", body()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut direct = body();
    direct["do"] = direct["do"][0]["if"]["then"].clone();
    let unguarded = lowerer()
        .lower(&tree("fanfare", direct))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (maximum, available, conditional) in [
        (6_u8, true, true),
        (7, true, true),
        (7, false, true),
        (6, true, false),
    ] {
        let mut game = position(
            if conditional { &guarded } else { &unguarded },
            maximum,
            available,
        );
        assert!(matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let play = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap();
        let active = maximum == 7 || !conditional;
        let result = if active {
            let CardPlay::Awaiting(question) = play else {
                panic!("mode declaration")
            };
            let InputDomain::Modes { modes, .. } = &question.domain else {
                panic!("modes")
            };
            assert_eq!(
                modes,
                &if available {
                    vec![ModeId(1), ModeId(2)]
                } else {
                    vec![ModeId(2)]
                }
            );
            if !available {
                let authority = format!("{game:?}");
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Modes(vec![ModeId(1)]),
                )
                .unwrap_err();
                assert_eq!(format!("{game:?}"), authority);
            }
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Modes(vec![ModeId(2)]),
                )
                .unwrap(),
            )
        } else {
            resolved(play)
        };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 20);
        if available {
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if active { 2_u8 } else { 0 }
            );
        }
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .count(),
            1
        );
    }
}
