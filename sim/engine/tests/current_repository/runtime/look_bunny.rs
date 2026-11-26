use super::*;

fn finish(game: &mut Game, mut play: CardPlay) -> (Activation, u8) {
    let mut choices = 0_u8;
    while let CardPlay::Awaiting(question) = play {
        choices = choices.checked_add(1).unwrap();
        assert!(
            game.internal_decision(View::P2).is_none(),
            "only the Look viewer chooses the hand card and remaining member"
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("private Look member")
        };
        let handles = domain
            .members
            .iter()
            .find(|card| card.card.as_deref() == Some("CP03-120"))
            .or_else(|| domain.members.first())
            .map(|card| vec![card.handle])
            .unwrap_or_default();
        play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap();
    }
    (resolved(play), choices)
}

fn ride(game: &mut Game) -> (Activation, u8) {
    let ability = selection_with_rules().current["CP03-054"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(payment) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("DrivePoint payment")
    };
    resolved(
        game.answer_internal_card(
            payment.actor,
            payment.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Drive gained")
    };
    let play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play)
}

#[test]
fn midnight_bunny_takes_one_privately_and_banishes_only_the_other_looked_card() {
    for count in [0_usize, 1, 3] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-054"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"point","card":"CSD03a-017"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!(
            [
                json!({"id":"taken","card":"CP03-120"}),
                json!({"id":"exiled","card":"CP03-121"}),
                json!({"id":"unseen","card":"BP01-171"})
            ]
            .into_iter()
            .take(count)
            .collect::<Vec<_>>()
        );
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "look-bunny",
        )
        .unwrap();
        let (result, choices) = ride(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        assert_eq!(view["objects"]["source"]["power"], 3_u8);
        assert_eq!(view["objects"]["source"]["hp"], 3_u8);
        assert_eq!(
            view["P1"]["hand"],
            if count > 0 {
                json!(["taken"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            view["P1"]["banish"],
            if count > 1 {
                json!(["exiled"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            view["P1"]["deck"],
            if count > 2 {
                json!(["unseen"])
            } else {
                json!([])
            }
        );
        assert_eq!(view["P1"]["cemetery"], json!([]));
        assert_ne!(view["game"]["ended"], true);
        if count > 1 {
            assert_eq!(
                choices, 1,
                "only the hand card is chosen; the other is banished directly"
            );
            assert!(result.progress.facts.iter().any(|fact| matches!(fact,
                Fact::Movement(movement) if movement.from.kind == ZoneKind::Deck && movement.movement.actual_to.is_some_and(|to| to.kind == ZoneKind::Banish)
            )), "the remaining card moves directly from the deck to banish");
        }
        let opponent = game.projection(View::P2).unwrap();
        let known = opponent["known_cards"].as_array().unwrap();
        assert!(
            !known
                .iter()
                .any(|card| card["id"] == "taken" || card["id"] == "unseen"),
            "the hand card and unobserved tail stay private"
        );
        if count > 1 {
            assert!(
                known.iter().any(|card| card["id"] == "exiled"),
                "only the banished member becomes public"
            );
        }
    }
}
