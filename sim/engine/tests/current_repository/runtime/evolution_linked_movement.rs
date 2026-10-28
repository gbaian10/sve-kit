use super::*;

fn position(seat: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([]);
    initial["players"][seat]["zones"]["field"] = json!([{"id":"base","card":"BP07-031"}]);
    initial["players"][seat]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP07-032"}]);
    initial["players"][seat]["zones"]["ex"] = json!([]);
    initial
}

#[test]
fn bp07_032_returns_the_base_after_evolution_release() {
    for seat in ["P1", "P2"] {
        let mut game = with_producer(
            &position(seat),
            Action::Destroy {
                objects: subjects(),
            },
        );
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["base"]["evolved_with"],
            "form"
        );
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        let departed = game.projection(View::Referee).unwrap();
        assert_eq!(departed[seat]["evolution"], json!([]));
        assert_eq!(departed[seat]["evolve_deck"], json!(["form"]));
        assert_eq!(departed["objects"]["form"]["face_up"], true);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = question.domain else {
            panic!("Lastword");
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, "BP07-032");
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["ex"], json!(["base"]));
        assert_eq!(state["objects"]["base"]["generation"], 2_u8);
        assert_eq!(state[seat]["evolve_deck"], json!(["form"]));
        assert_eq!(state["objects"]["form"]["generation"], 2_u8);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn bp01_155_moves_an_evolved_opponent_to_ex_and_returns_its_form_without_lastword() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"urd","card":"BP01-155"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{
            "id":"base","card":"BP07-031","state":{"evolved":true,"evolved_with":"form","damage":1_u8,"counters":{"old":2_u8}}
        }]);
        initial["players"][opponent]["zones"]["evolution"] =
            json!([{"id":"form","card":"BP07-032"}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card(
                "urd",
                PlayerId {
                    seat: seat_number(seat)
                }
            )
            .unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = pending.domain else {
            panic!("fanfare");
        };
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("target");
        };
        let InputDomain::Targets(domain) = target.domain else {
            panic!("target domain");
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[opponent]["ex"], json!(["base"]));
        assert_eq!(state[opponent]["evolve_deck"], json!(["form"]));
        assert_eq!(state[opponent]["evolution"], json!([]));
        assert_eq!(state["objects"]["base"]["damage"], 0_u8);
        assert_eq!(state["objects"]["base"]["counters"], json!({}));
        assert!(state["objects"]["base"]["evolved_with"].is_null());
        assert_eq!(state["objects"]["form"]["face_up"], true);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
