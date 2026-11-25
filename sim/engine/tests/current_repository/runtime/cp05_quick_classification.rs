#![expect(
    clippy::panic,
    reason = "A different decision invalidates the printed-card scenario."
)]

use super::*;

fn resolve_fanfare(game: &mut Game, holder: &str) -> CardPlay {
    assert!(matches!(
        game.play_internal_card(holder, PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected fanfare");
    };
    assert_eq!(options.len(), 1);
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn wamdus_selects_quick_spell_and_quick_follower_and_mills_quick_amulet() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-SL11"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"quick_spell","card":"BP01-065"},
        {"id":"quick_follower","card":"BP10-101"},
        {"id":"quick_amulet","card":"BP09-034"}
    ]);
    let mut game = game(&initial);
    let choice = awaiting(resolve_fanfare(&mut game, "source"));
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("expected private Look selection");
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    assert_eq!(
        domain
            .members
            .iter()
            .filter_map(|option| option.card.as_deref())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["BP01-065", "BP10-101"])
    );
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Targets(domain.members.iter().map(|option| option.handle).collect()),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["quick_amulet"]));
    assert_eq!(
        state["P1"]["hand"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["quick_spell", "quick_follower"])
    );
}

#[test]
fn cassius_counts_only_followers_with_printed_quick_activations_in_cemetery() {
    for (quick_followers, pp_after_fanfare) in [(6, 0_u8), (7, 3_u8)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":7_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-043"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut cemetery = vec![
            json!({"id":"quick_spell","card":"BP01-065"}),
            json!({"id":"ordinary_follower","card":"BP01-037"}),
        ];
        cemetery.extend(
            (0..quick_followers)
                .map(|n| json!({"id":format!("quick-follower-{n}"),"card":"BP10-101"})),
        );
        initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
        let mut game = game(&initial);
        resolved(resolve_fanfare(&mut game, "source"));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], pp_after_fanfare);
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn hakutaku_activates_on_opponents_turn_and_cannot_repeat_in_the_same_window() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":2_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-GBF78"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let choice = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(choice.actor, PlayerId { seat: 1 });
    let InputDomain::Quick { options, .. } = &choice.domain else {
        panic!("expected Quick window");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "CP05-GBF78");
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap(),
    );
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let next = game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(&next.domain, InputDomain::Quick { options, .. } if options.is_empty()));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
}
