use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, question, ub};
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn credited_position() -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP04-026"},
        {"id":"spell","card":"BP01-125"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"peco","card":"CP04-019"}, {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"attacker","card":"BP01-037","state":{"power":1_u8}},
        {"id":"zero","card":"BP01-037","state":{"power":0_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"power":0_u8,"acted":true}}
    ]);
    game(&initial)
}

#[test]
fn creditta_searches_discounted_pecorine_and_increases_ability_and_combat_damage() {
    let mut game = credited_position();
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let search = question(pending(&mut game, "CP04-026"));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("Pecorine search");
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "CP04-019");
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    let entry = game
        .play_internal_card("peco", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(finish(&mut game, entry), CardPlay::Entered(_)));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        6_u8
    );
    let fee = question(pending(&mut game, "CP04-019"));
    assert!(matches!(fee.domain, InputDomain::OptionalCost));
    assert!(matches!(
        game.answer_internal_card(fee.actor, fee.token, Answer::OptionalCost(false))
            .unwrap(),
        CardPlay::Cancelled
    ));
    finish_checks(&mut game);
    let ability = selection().current["CP04-026"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let ub_play = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    ub(&resolved(finish(&mut game, ub_play)));
    finish_checks(&mut game);
    let spell_play = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    let target = question(spell_play);
    let InputDomain::Targets(targets) = &target.domain else {
        panic!("spell target");
    };
    let follower = targets
        .members
        .iter()
        .find(|member| member.card.is_some())
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![follower]))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u8
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        15_u8
    );
    finish_checks(&mut game);
    for (attacker, hp) in [("zero", 15_u8), ("attacker", 12_u8)] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        loop {
            finish_checks(&mut game);
            let Some(quick) = game.internal_quick_choice().unwrap() else {
                break;
            };
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            hp
        );
    }
}
