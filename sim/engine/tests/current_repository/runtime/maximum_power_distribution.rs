use super::*;
use sve_engine::game::internal::DamageShare;

#[test]
fn blade_dance_distributes_the_maximum_friendly_power_even_when_tied() {
    let mut initial = position_with_source("BP12-P06", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"one","card":"BP01-171","state":{"power":5_u8}},
        {"id":"two","card":"BP01-171","state":{"power":5_u8}},
        {"id":"three","card":"BP01-171","state":{"power":3_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy-one","card":"BP01-171","state":{"hp":8_u8,"max_hp":8_u8}},
        {"id":"enemy-two","card":"BP01-171","state":{"hp":8_u8,"max_hp":8_u8}}
    ]);
    let mut board = game(&initial);
    let target = await_card_choice(
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(targets) = &target.domain else {
        panic!("formal targets");
    };
    let target_answer = Answer::Targets(targets.members.iter().map(|member| member.handle).collect());
    let allocation = await_card_choice(
        board
            .answer_internal_card(target.actor, target.token, target_answer)
            .unwrap(),
    );
    let InputDomain::Distribution(distribution) = &allocation.domain else {
        panic!("damage allocation");
    };
    assert_eq!(distribution.total, Nat(5));
    let distribution_answer = Answer::Distribution(
        distribution
            .members
            .iter()
            .zip([Nat(2), Nat(3)])
            .map(|(member, amount)| DamageShare { handle: member.handle, amount })
            .collect(),
    );
    drop(resolved(
        board
            .answer_internal_card(allocation.actor, allocation.token, distribution_answer)
            .unwrap(),
    ));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy-one"]["hp"], 6_u8);
    assert_eq!(state["objects"]["enemy-two"]["hp"], 5_u8);
}
