use super::*;
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
#[test]
fn bp22_017_whole_card_creates_the_committed_departure_count_and_uses_one_turn_quota() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"dungeon","card":"BP22-017"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"a","card":"BP22-T05"},{"id":"b","card":"BP22-T05"}]);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"BP22-T05"}]);
    let objects = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::All,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(2),
                maximum: Some(Nat(2)),
            },
            text_quantifier: TextQuantifier::All,
        },
        zone: ZoneKind::Ex,
        view: SelectionView::Current {},
        binding: Binding::new(71, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let mut game = with_producer(&initial, Action::Banish { objects });
    activate(&mut game, "P1");
    finish(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("native departure trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "two simultaneous own departures queue one batch trigger"
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
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["ex"].as_array().unwrap().len(),
        2,
        "the frozen own count creates two Fairies after EX became empty"
    );
    assert!(
        after["P2"]["ex"].as_array().unwrap().is_empty(),
        "foreign EX departures do not add to the count"
    );
    activate(&mut game, "P1");
    finish(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the same source instance has consumed this turn's quota"
    );
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the second batch creates no replacement Fairies"
    );
}
