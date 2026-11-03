use super::union_burst_batch1::{build, enter, evolve, finish, pending, position, ub};
use super::*;

fn aliases(representative: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[representative].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(number, _)| number.clone())
        .collect()
}
fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn activate(game: &mut Game, source: &str, card: &str) -> CardPlay {
    let definition = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    game.activate_internal_card(source, definition, PlayerId { seat: 1 })
        .unwrap()
}
fn trigger(game: &mut Game, card: &str) {
    let play = pending(game, card);
    assert!(
        matches!(finish(game, play), CardPlay::Resolved(_)),
        "the printed observer resolves"
    );
    settle(game);
}
#[test]
fn mitsuki_and_ninon_observe_each_others_ub_but_not_their_own() {
    for card in aliases("CP04-033") {
        let mut initial = position(&card, "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ninon","card":"CP04-035"}]);
        let mut game = build(&initial);
        enter(&mut game);
        let fanfare = pending(&mut game, &card);
        ub(&resolved(finish(&mut game, fanfare)));
        trigger(&mut game, "CP04-035");
        let after_fanfare = game.projection(View::Referee).unwrap();
        assert_eq!(
            after_fanfare["objects"]["enemy"]["power"], 8_u8,
            "Mitsuki's own UB excludes her observer"
        );
        assert_eq!(after_fanfare["objects"]["ninon"]["power"], 3_u8);
        let ninon_ub = activate(&mut game, "ninon", "CP04-035");
        ub(&resolved(finish(&mut game, ninon_ub)));
        trigger(&mut game, &card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 7_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 12_u8);
        assert_eq!(
            state["objects"]["ninon"]["power"], 3_u8,
            "Ninon's own UB does not buff her again"
        );
        assert_eq!(state["objects"]["ninon"]["acted"], false);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
#[test]
fn ninon_stands_after_payment_and_damage_and_cannot_use_the_same_ub_twice_in_a_turn() {
    for card in aliases("CP04-035") {
        let mut initial = position(&card, "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other","card":card}));
        let mut game = build(&initial);
        let play = activate(&mut game, "source", &card);
        ub(&resolved(finish(&mut game, play)));
        trigger(&mut game, &card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["objects"]["source"]["acted"], false);
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert_eq!(state["objects"]["other"]["power"], 3_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 15_u8);
        let definition = selection_with_rules().current[&card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        game.activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap_err();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
#[test]
fn nanaka_discards_for_her_own_ub_and_recovers_one_priconne_spell_per_turn_from_other_ub() {
    for card in aliases("CP04-052") {
        let mut initial = position(&card, "hand");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fee","card":"CP04-036"}));
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ninon","card":"CP04-035"},{"id":"kuka","card":"CP04-051"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"second","card":"CP04-036"}]);
        let mut game = build(&initial);
        enter(&mut game);
        let fanfare = pending(&mut game, &card);
        ub(&resolved(finish(&mut game, fanfare)));
        trigger(&mut game, "CP04-035");
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("fee"))
        );
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "Nanaka does not observe her own UB"
        );
        let ninon_ub = activate(&mut game, "ninon", "CP04-035");
        ub(&resolved(finish(&mut game, ninon_ub)));
        trigger(&mut game, &card);
        let recovered_state = game.projection(View::Referee).unwrap();
        let recovered = ["fee", "second"]
            .into_iter()
            .filter(|id| {
                recovered_state["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id))
            })
            .count();
        assert_eq!(recovered, 1);
        let kuka_ub = activate(&mut game, "kuka", "CP04-051");
        ub(&resolved(finish(&mut game, kuka_ub)));
        trigger(&mut game, "CP04-035");
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "Nanaka's shared trigger quota is already consumed"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            ["fee", "second"]
                .into_iter()
                .filter(|id| state["P1"]["ex"].as_array().unwrap().contains(&json!(id)))
                .count(),
            1
        );
        assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    }
}

#[test]
fn an_evolved_ub_holder_is_still_a_follower_for_mitsukis_subscription() {
    for card in aliases("CP04-050") {
        let mut initial = position(&card, "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"observer","card":"CP04-033"}));
        let mut game = evolve(&card, initial);
        let evolved_ub = pending(&mut game, &card);
        ub(&resolved(finish(&mut game, evolved_ub)));
        trigger(&mut game, "CP04-033");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 7_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 17_u8);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
