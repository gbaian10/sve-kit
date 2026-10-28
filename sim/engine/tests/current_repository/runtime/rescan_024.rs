#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real-card scenario."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"}
    ]);
    initial
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the real card's trigger");
    };
    assert_eq!(
        options.len(),
        1_usize,
        "exactly one pending trigger is expected"
    );
    assert_eq!(
        options[0].card, card,
        "the pending trigger must belong to the acceptance card"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
}

#[test]
fn bp17_085_fanfare_damages_only_its_controller() {
    let mut initial = position("BP17-085", "hand");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    trigger(&mut game, "BP17-085");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["field"], json!(["source"]));
}

#[test]
fn bp17_t01_lastword_damages_controller_but_banish_does_not() {
    for destroyed in [true, false] {
        let action = if destroyed {
            Action::Destroy {
                objects: subjects(),
            }
        } else {
            movement(OwnerZone::Banish)
        };
        let mut game = with_producer(&position("BP17-T01", "field"), action);
        activate(&mut game, "P1");
        game.resume_internal_card().unwrap();
        if destroyed {
            trigger(&mut game, "BP17-T01");
        } else {
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if destroyed { 19_u8 } else { 20_u8 }
        );
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["field"], json!([]));
    }
}

#[test]
fn bp17_013_lastword_draws_exactly_one_real_card() {
    let mut game = with_producer(
        &position("BP17-013", "field"),
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game, "BP17-013");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["deck"], json!(["second"]));
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(state["P2"]["hand"], json!([]));
}

#[test]
fn bp17_t09_created_on_field_chooses_ward_then_draws() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"}
    ]);
    let action = Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId("エスカマリの助言者".into()),
            name: Expr::Literal(CardName("エスカマリの助言者".into())),
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    };
    let mut game = with_producer(&initial, action);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("producer", AbilityId(1), actor())
        .unwrap()
    else {
        panic!("Ward entry posture");
    };
    assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::WardEntry(true))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    trigger(&mut game, "BP17-T09");
    let state = game.projection(View::Referee).unwrap();
    let tokens = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| &state["objects"][id.as_str().unwrap()])
        .filter(|object| object["card"] == "BP17-T09")
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 1_usize);
    assert_eq!(tokens[0]["acted"], true);
    assert_eq!(tokens[0]["keywords"], json!(["守護"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["deck"], json!(["second"]));
    assert_eq!(state["P2"]["hand"], json!([]));
}
