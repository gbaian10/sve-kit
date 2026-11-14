use super::*;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn bp21_023_whole_card_gains_storm_and_evolves_only_after_ability_entry() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP21-023"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-024"}]);
    let mut enter =
        event_cemetery::move_named(ZoneKind::Ex, OwnerZone::Cemetery, "飛躍の双剣使い");
    let Action::Move {
        objects: Selector::Zone { players, .. },
        movement,
        ..
    } = &mut enter
    else {
        panic!("entry producer");
    };
    *players = PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::One,
    };
    movement.to = MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Field,
        },
    };
    let mut game = with_producer(&initial, enter);
    activate(&mut game, "P1");
    finish(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare");
    };
    assert_eq!(
        options.len(),
        1,
        "the entered follower supplies its Fanfare"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走")),
        "ability entry enables the printed Storm clause"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the retained actual entry method qualifies its intrinsic evolution"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["evolved"],
        true,
        "the real form is installed"
    );
}
