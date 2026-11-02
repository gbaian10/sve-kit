#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate printed cost scenarios."
)]
use super::*;

fn position(card: &str, acted: bool) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":card,"state":{"acted":acted}}]);
    value["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    value["players"]["P2"]["zones"]["field"] = json!([]);
    value
}

fn pending(game: &mut Game) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn pay(game: &mut Game, play: CardPlay, accept: bool) -> Activation {
    let CardPlay::Awaiting(cost) = play else {
        panic!("resolution payment")
    };
    assert_eq!(
        cost.domain,
        InputDomain::OptionalCost,
        "payment is a resolution decision"
    );
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId { seat: 2 },
        cost.token,
        Answer::OptionalCost(accept),
    )
    .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "a wrong actor cannot change the waiting authority"
    );
    let activation = resolved(
        game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(accept))
            .unwrap(),
    );
    assert!(
        activation
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::AbilityPlayed { .. })),
        "declining a resolution payment preserves the ability play"
    );
    activation
}

#[test]
fn bp09_092_healing_trigger_acts_only_a_standing_source_and_can_be_declined() {
    for card in ["BP09-092", "PR-320"] {
        for (acted, accept) in [(false, true), (false, false), (true, true)] {
            let mut game = with_producer(
                &position(card, acted),
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1)),
                    },
                },
            );
            activate(&mut game, "P1");
            let CardPlay::Awaiting(target) = pending(&mut game) else {
                panic!("target before resolution payment")
            };
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("recipient")
            };
            let play = game
                .answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            pay(&mut game, play, accept);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["acted"],
                acted || accept,
                "only a paid standing source becomes acted"
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if accept && !acted { 18_i64 } else { 20_i64 },
                "only successful payment enables the damage"
            );
        }
    }
}

#[test]
fn bp21_109_ability_damage_trigger_sacrifices_before_heal_and_draw_or_declines() {
    for (card, accept) in [("BP21-109", true), ("BP21-109", false), ("BP21-P61", true)] {
        let mut game = with_producer(
            &position(card, false),
            Action::Damage {
                recipients: RecipientSet::Leaders {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::All,
                    },
                },
                amount: DamageAmount::Uniform {
                    amount: Expr::Literal(Int(1)),
                },
                flags: DamageFlags::Ability,
                source: ObjectView::SourceInformation {},
                batch: BatchPolicy::Instruction {},
            },
        );
        activate(&mut game, "P1");
        let play = pending(&mut game);
        pay(&mut game, play, accept);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if accept { 23_i64 } else { 19_i64 },
            "the paid effect heals four after ability damage"
        );
        assert_eq!(
            state["P1"]["hand"],
            if accept { json!(["drawn"]) } else { json!([]) },
            "draw requires the source sacrifice"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if accept { json!(["source"]) } else { json!([]) },
            "declining leaves the source in the field"
        );
    }
}

#[test]
fn bp01_162_checks_life_before_optional_act_sacrifice_and_keeps_fanfare() {
    for (card, starting_life) in [("BP01-162", 9_i64), ("PR-027", 10)] {
        let mut initial = position(card, false);
        initial["players"]["P1"]["leader"]["life"] = json!(starting_life);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"},{"id":"draw3","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_i64,"max_hp":10_i64}}]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "path-to-purgatory",
        )
        .unwrap();
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the amulet enters before its separate fanfare"
        );
        resolved(pending(&mut game));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            json!(["draw1", "draw2", "draw3"]),
            "the full printed fanfare draws three"
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            starting_life - 3,
            "the fanfare deals three to its controller"
        );
        game.resume_internal_card().unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let play = pending(&mut game);
        if starting_life == 9 {
            pay(&mut game, play, true);
        } else {
            resolved(play);
        }
        let completed = game.projection(View::Referee).unwrap();
        assert_eq!(
            completed["objects"]["enemy"]["damage"],
            if starting_life == 9 { 6_i64 } else { 0_i64 },
            "the life condition gates the optional sacrifice and damage"
        );
        assert_eq!(
            completed["P1"]["cemetery"],
            if starting_life == 9 {
                json!(["source"])
            } else {
                json!([])
            },
            "only the qualifying paid trigger sacrifices the source"
        );
    }
}
