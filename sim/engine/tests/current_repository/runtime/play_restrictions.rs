use super::*;
use sve_engine::game::internal::DamageShare;

#[expect(
    clippy::panic,
    reason = "Unexpected runtime choice invalidates the scenario."
)]
fn from_ex_case(card: &str, remaining_pp: u8) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    let mut objects = subjects();
    let Selector::Zone { players, .. } = &mut objects else {
        panic!("field");
    };
    players.relation = PlayerRelation::SelfPlayer;
    players.text_quantifier = TextQuantifier::Implicit;
    players.cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    let mut game = with_producer(&initial, Action::Destroy { objects });
    let CardPlay::Entered(_) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("follower entry");
    };
    game.resume_internal_card().unwrap();
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(
        entered["P1"]["pp"]["current"], remaining_pp,
        "pay the printed cost once"
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Lastword");
    };
    assert_eq!(
        options.len(),
        1,
        "only this card contributes a pending ability"
    );
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the Lastword"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let returned = game.projection(View::Referee).unwrap();
    assert_eq!(
        returned["P1"]["ex"],
        json!(["source"]),
        "Lastword follows its own successful move"
    );
    assert_eq!(
        returned["P1"]["pp"]["current"], remaining_pp,
        "Lastword costs no additional PP"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no pending ability may mask the EX play restriction"
    );
    let before = format!("{game:?}");
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "forbidden play preserves the complete authority"
    );
}

#[test]
fn bp19_066_and_alias_play_from_hand_return_by_lastword_and_cannot_replay_from_ex() {
    for card in ["BP19-066", "BP19-P19"] {
        from_ex_case(card, 9);
    }
}

#[test]
fn bp19_064_and_alias_keep_evolution_and_lastword_but_cannot_replay_from_ex() {
    for card in ["BP19-064", "BP19-P17"] {
        from_ex_case(card, 8);
    }
}

#[test]
fn bp11_034_and_alias_forbid_own_turn_then_quick_moves_own_follower_to_ex() {
    for card in ["BP11-034", "BP11-P08"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"retreat","card":card}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own-ally","card":"BP01-037"}]);
        let mut own_turn = game(&initial);
        let before = format!("{own_turn:?}");
        own_turn
            .play_internal_card("retreat", PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(format!("{own_turn:?}"), before);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"retreat","card":card}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        assert_eq!(options.len(), 1);
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap()
        else {
            panic!("target");
        };
        resolved(
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["ex"], json!(["ally"]));
        assert_eq!(view["P2"]["field"], json!([]));
        assert_eq!(view["P2"]["cemetery"], json!(["retreat"]));
        assert_eq!(view["P2"]["pp"]["current"], 10_u8);
    }
}

#[test]
fn bp06_106_and_alias_quick_require_remaining_two_pp_for_heal_and_draw() {
    for (card, pp, zone) in [("BP06-106", 1_u8, "hand"), ("BP06-P30", 2_u8, "ex")] {
        let mut initial = draw_position();
        initial["players"]["P2"]["pp"]["current"] = json!(pp);
        initial["players"]["P2"]["leader"]["life"] = json!(17_u8);
        initial["players"]["P2"]["zones"][zone] = json!([{"id":"concentrate","card":card}]);
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        assert_eq!(options.len(), 1);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["leader"]["life"],
            if pp >= 2 { 18_u8 } else { 17_u8 }
        );
        assert_eq!(
            view["P2"]["hand"],
            if pp >= 2 { json!(["drawn"]) } else { json!([]) }
        );
        assert_eq!(view["P2"]["pp"]["current"], pp);
        assert_eq!(view["P2"]["cemetery"], json!(["concentrate"]));
    }
}

#[test]
fn bp08_092_family_quick_distributes_four_damage_to_two_opposing_followers() {
    for card in ["BP08-092", "EBD04-010", "PR-258"] {
        let mut initial = draw_position();
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"spell","card":card}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        assert_eq!(options.len(), 1);
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap()
        else {
            panic!("targets");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("target domain");
        };
        assert_eq!(domain.members.len(), 2);
        let handles = domain
            .members
            .iter()
            .map(|member| member.handle)
            .collect::<Vec<_>>();
        let CardPlay::Awaiting(allocation) = game
            .answer_internal_card(target.actor, target.token, Answer::Targets(handles.clone()))
            .unwrap()
        else {
            panic!("allocation");
        };
        assert!(
            matches!(&allocation.domain, InputDomain::Distribution(distribution) if distribution.total == Nat(4))
        );
        let result = resolved(
            game.answer_internal_card(
                allocation.actor,
                allocation.token,
                Answer::Distribution(vec![
                    DamageShare {
                        handle: handles[0],
                        amount: Nat(2),
                    },
                    DamageShare {
                        handle: handles[1],
                        amount: Nat(2),
                    },
                ]),
            )
            .unwrap(),
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Damage(_)))
                .count(),
            2
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["first"]["damage"], 2_u8);
        assert_eq!(view["objects"]["second"]["damage"], 2_u8);
        assert_eq!(view["P2"]["pp"]["current"], 8_u8);
        assert_eq!(view["P2"]["cemetery"], json!(["spell"]));
    }
}
