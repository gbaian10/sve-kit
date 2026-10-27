#![expect(
    clippy::panic,
    reason = "An unexpected pending domain means the scenario did not reach the Fanfare."
)]

use super::*;

fn position(seat: &str, enemies: bool) -> Value {
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["evolve_deck"] =
        json!([{"id":"advance","card":"BP19-P32","owner":seat}]);
    initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"][opponent]["zones"]["field"] = if enemies {
        json!([
            {"id":"enemy-1","card":"BP01-037"},
            {"id":"enemy-2","card":"BP01-037"}
        ])
    } else {
        json!([])
    };
    initial
}

fn advance_entry() -> Action {
    Action::Move {
        objects: Selector::Zone {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                text_quantifier: TextQuantifier::All,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
            },
            zone: ZoneKind::Evolve,
            view: SelectionView::Current {},
            binding: Binding::new(50, ScopeId(1)),
            predicate: Box::new(Expr::Literal(Bool(true))),
        },
        movement: MoveSpec {
            to: MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                },
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

fn advance_and_resolve_fanfare(game: &mut Game, seat: &str) {
    let ward_question = match game
        .activate_internal_card(
            "producer",
            AbilityId(1),
            PlayerId {
                seat: seat_number(seat),
            },
        )
        .unwrap()
    {
        CardPlay::Awaiting(question) => question,
        CardPlay::Resolved(result) => {
            panic!("expected Ward entry decision before source resolution, got {result:?}")
        }
        CardPlay::Entered(entry) => {
            panic!("expected effect entry to ask about Ward before completing, got {entry:?}")
        }
        CardPlay::Cancelled => {
            panic!("expected the effect entry to remain valid, but it cancelled")
        }
        CardPlay::Continued(step) => {
            panic!("expected Ward entry decision before continuing a rule step, got {step:?}")
        }
    };
    assert_eq!(
        ward_question.actor.seat,
        seat_number(seat),
        "the entering controller answers the Ward entry decision"
    );
    assert!(
        matches!(
            &ward_question.domain,
            InputDomain::WardEntry { card, zone, .. }
                if card == "BP19-P32" && zone == "evolve_deck"
        ),
        "Ward prompt must identify BP19-P32 in the evolve deck"
    );
    game.answer_internal_card(
        ward_question.actor,
        ward_question.token,
        Answer::WardEntry(false),
    )
    .unwrap();
    game.resume_internal_card().unwrap();

    let fanfare_question = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(
        fanfare_question.actor.seat,
        seat_number(seat),
        "the entering controller chooses its Fanfare"
    );
    let InputDomain::Pending(options) = &fanfare_question.domain else {
        panic!("pending Fanfare");
    };
    let [option] = options.as_slice() else {
        panic!("one Fanfare");
    };
    assert_eq!(
        option.card, "BP19-P32",
        "the true current card owns Fanfare"
    );
    assert_eq!(
        option.line, 2,
        "the destroy instruction keeps its printed line"
    );
    assert!(
        matches!(
            game.answer_internal_card(
                fanfare_question.actor,
                fanfare_question.token,
                Answer::Pending(option.handle),
            )
            .unwrap(),
            CardPlay::Resolved(_)
        ),
        "answering the card's only Fanfare resolves its body"
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "all Ward and Fanfare questions are resolved"
    );
}

#[test]
fn bp19_p32_fanfare_destroys_all_opposing_followers_only() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut game = with_producer(&position(seat, true), advance_entry());
        advance_and_resolve_fanfare(&mut game, seat);

        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["field"], json!(["ally", "producer", "advance"]));
        assert_eq!(state[opponent]["field"], json!([]));
        assert_eq!(state[opponent]["cemetery"], json!(["enemy-1", "enemy-2"]));
    }
}

#[test]
fn bp19_p32_fanfare_resolves_with_no_opposing_followers() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut game = with_producer(&position(seat, false), advance_entry());
        advance_and_resolve_fanfare(&mut game, seat);

        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["field"], json!(["ally", "producer", "advance"]));
        assert_eq!(state[opponent]["field"], json!([]));
        assert_eq!(state[opponent]["cemetery"], json!([]));
    }
}
