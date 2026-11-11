use super::*;

fn experiment_position(number: &str, unpaid: &str, zone: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP19-042"}, {"id":"spare","card":"BP19-040"},
        {"id":"unpaid","card":unpaid}
    ]);
    let experiment = json!({"id":"material","card":number});
    if zone == "hand" {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(experiment);
    } else {
        initial["players"]["P1"]["zones"]["ex"] = json!([experiment]);
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    game(&initial)
}

#[test]
fn experiment_all_prints_draw_then_discard_when_used_from_hand_or_ex() {
    for (number, unpaid, zone) in [("BP19-048", "PR-518", "hand"), ("PR-518", "BP19-048", "ex")] {
        let mut board = experiment_position(number, unpaid, zone);
        let ability = selection().current["BP19-042"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.qualification == AbilityQualification::Fusion)
            .unwrap()
            .node
            .id;
        let CardPlay::Awaiting(material_choice) = board
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("expected Fusion material choice");
        };
        let InputDomain::CostMaterials(material_domain) = &material_choice.domain else {
            panic!("expected Fusion cost domain");
        };
        assert!(
            material_domain
                .members
                .iter()
                .any(|member| member.card.as_deref() == Some(unpaid)),
            "the unselected printing is also a legal material"
        );
        let handle = material_domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some(number))
            .unwrap()
            .handle;
        let play = board
            .answer_internal_card(
                material_choice.actor,
                material_choice.token,
                Answer::CostMaterials(vec![handle]),
            )
            .unwrap();
        resolved(play);
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let question = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("expected the paid Experiment's pending ability");
        };
        assert_eq!(options.len(), 1, "the actual paid Experiment triggers once");
        assert_eq!(options[0].card, number, "the event names the paid printing");
        assert_eq!(options[0].line, 1, "Fusion runs the first printed ability");
        let CardPlay::Awaiting(discard) = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("draw precedes the private discard choice");
        };
        let InputDomain::Targets(discard_domain) = &discard.domain else {
            panic!("expected hand discard domain");
        };
        assert!(
            board.internal_decision(View::P2).is_none(),
            "the opponent cannot see the discard choices"
        );
        let drawn = discard_domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-171"))
            .unwrap()
            .handle;
        resolved(
            board
                .answer_internal_card(discard.actor, discard.token, Answer::Targets(vec![drawn]))
                .unwrap(),
        );
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["cemetery"],
            json!(["material", "drawn"]),
            "the drawn card is a legal choice for the same effect's discard"
        );
        assert_eq!(
            after["P1"]["hand"],
            json!(["spare", "unpaid"]),
            "the unselected printing stays in hand and only one card is discarded"
        );
    }
}
