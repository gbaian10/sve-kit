#![expect(
    clippy::panic,
    reason = "The real card requires its Ward posture and printed either choices."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn choose(game: &mut Game, play: CardPlay, index: usize) {
    let CardPlay::Awaiting(question) = play else {
        panic!("the token name must be chosen during resolution");
    };
    let InputDomain::Modes {
        modes,
        count,
        cardinality,
    } = &question.domain
    else {
        panic!("the printed either requires a mode");
    };
    assert_eq!(*count, Nat(1), "one token name is required");
    assert_eq!(cardinality.minimum, Nat(1), "the choice cannot be declined");
    assert_eq!(modes.len(), 2, "both printed names are available");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![modes[index]]),
        )
        .unwrap(),
    );
}

fn assert_tokens(game: &Game, hellflame: usize, dragon: usize) {
    let view = game.projection(View::Referee).unwrap();
    let field = view["P1"]["field"].as_array().unwrap();
    for (number, count, power, hp) in [
        ("BP02-T06", hellflame, 4_u8, 3_u8),
        ("BP01-T11", dragon, 5, 5),
    ] {
        let tokens = field
            .iter()
            .map(|id| &view["objects"][id.as_str().unwrap()])
            .filter(|card| card["card"] == number)
            .collect::<Vec<_>>();
        assert_eq!(
            tokens.len(),
            count,
            "only the selected real token is created"
        );
        for token in tokens {
            assert_eq!(token["power"], power, "printed token power is preserved");
            assert_eq!(token["hp"], hp, "printed token health is preserved");
            if number == "BP02-T06" {
                assert!(
                    token["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("突進")),
                    "the complete Hellflame Dragon has Rush"
                );
            }
        }
    }
    assert_eq!(
        view["P2"]["field"],
        json!([]),
        "the opponent receives no token"
    );
    assert_eq!(
        view["P1"]["ex"],
        json!([]),
        "both abilities create on the field"
    );
}

#[test]
fn bp18_074_fanfare_and_paid_act_choose_independently_between_real_dragons() {
    for first in [0_usize, 1] {
        let mut state = draw_position();
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP18-074"}]);
        let mut game = game(&state);
        let CardPlay::Awaiting(ward) = game.play_internal_card("source", actor()).unwrap() else {
            panic!("the real mother has Ward entry");
        };
        assert!(
            matches!(ward.domain, InputDomain::WardEntry { .. }),
            "Ward precedes fanfare resolution"
        );
        assert!(
            matches!(
                game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the mother enters standing"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("the actual fanfare is pending");
        };
        assert_eq!(options.len(), 1, "one fanfare is pending");
        assert_eq!(
            options[0].card, "BP18-074",
            "the actual mother supplies the ability"
        );
        let play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        choose(&mut game, play, first);
        assert_tokens(&game, usize::from(first == 0), usize::from(first == 1));
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["pp"]["current"], 4_u8,
            "the real follower costs six PP"
        );
        assert_eq!(
            before["objects"]["source"]["acted"], false,
            "the Ward choice left the source standing"
        );
        game.resume_internal_card().unwrap();
        let ability = selection().current["BP18-074"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let activation = game
            .activate_internal_card("source", ability, actor())
            .unwrap();
        choose(&mut game, activation, 1 - first);
        assert_tokens(&game, 1, 1);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"], 2_u8,
            "activation pays another two PP"
        );
        assert_eq!(
            after["objects"]["source"]["acted"], true,
            "activation acts the source as a cost"
        );
        game.resume_internal_card().unwrap();
        assert!(
            game.activate_internal_card("source", ability, actor())
                .is_err(),
            "an already acted source cannot pay its act cost again"
        );
        assert_tokens(&game, 1, 1);
    }
}
