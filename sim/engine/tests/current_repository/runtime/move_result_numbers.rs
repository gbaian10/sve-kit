use super::event_cemetery::finish_checks;
use super::*;

#[test]
fn circle_wizard_pays_five_and_heals_for_the_actual_ex_cards_original_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-050"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"placed","card":"BP01-037","state":{"cost":0_u8}},
        {"id":"tail","card":"BP01-171"}
    ]);
    let mut board = game(&initial);
    assert!(matches!(
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish_checks(&mut board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("the real Fanfare");
    };
    assert_eq!(options.len(), 1_usize);
    assert_eq!(options[0].card, "BP13-050");
    let result = resolved(
        board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::LifeChange(_)))
            .count(),
        1_usize
    );
    finish_checks(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 5_u8);
    assert_eq!(
        view["P1"]["leader"]["life"], 25_u8,
        "the five printed cost survives the temporary zero-cost state"
    );
    assert_eq!(view["P1"]["ex"], json!(["placed"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
    assert_eq!(view["objects"]["placed"]["generation"], 1_u8);
}

#[test]
fn both_deep_sea_approach_prints_pay_seven_choose_only_a_marine_follower_and_damage_each_enemy() {
    for number in ["BP20-066", "PR-548"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"marine","card":"BP20-060"},
            {"id":"wrong-trait","card":"BP01-171"},
            {"id":"wrong-kind","card":"BP20-066"},
            {"id":"fourth","card":"BP01-034"},
            {"id":"fifth","card":"BP01-037"},
            {"id":"tail","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy1","card":"BP01-171","state":{"hp":10_u8}},
            {"id":"enemy2","card":"BP01-171","state":{"hp":10_u8}}
        ]);
        let mut board = game(&initial);
        let CardPlay::Awaiting(q) = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("Look choice");
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("marine follower candidates");
        };
        assert_eq!(domain.cardinality.minimum, Nat(0));
        assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
        assert_eq!(domain.members.len(), 1_usize);
        assert_eq!(domain.members[0].card.as_deref(), Some("BP20-060"));
        assert!(board.internal_decision(View::P2).is_none());
        let CardPlay::Awaiting(q) = board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap()
        else {
            panic!("ordered remainder");
        };
        let InputDomain::Order(cards) = &q.domain else {
            panic!("private deck-bottom order");
        };
        assert_eq!(cards.len(), 4_usize);
        let result = resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Order(cards.iter().rev().map(|c| c.handle).collect()),
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
            2_usize
        );
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 3_u8);
        assert_eq!(view["P1"]["field"], json!(["marine"]));
        assert_eq!(view["objects"]["marine"]["hp"], 2_u8);
        assert_eq!(view["objects"]["enemy1"]["hp"], 8_u8);
        assert_eq!(view["objects"]["enemy2"]["hp"], 8_u8);
        assert_eq!(
            view["P1"]["deck"],
            json!(["tail", "fifth", "fourth", "wrong-kind", "wrong-trait"])
        );
        assert_eq!(view["P1"]["cemetery"], json!(["source"]));
    }
}
