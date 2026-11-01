#![expect(
    clippy::panic,
    reason = "These real evolution scenarios have only the printed capacity choice."
)]
use super::*;

fn position(form: &str, occupied: usize) -> Value {
    let mut state = draw_position();
    let mut field = vec![json!({"id":"source","card":"SD02-003"})];
    field.extend((0..occupied).map(|n| json!({"id":format!("old-{n}"),"card":"BP17-T11"})));
    state["players"]["P1"]["zones"]["field"] = json!(field);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    state
}

fn evolve_and_trigger(game: &mut Game, form: &str) -> CardPlay {
    let action = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        action.outcome, "resolved",
        "the real Floral Fencer evolves through its printed one-PP action"
    );
    assert!(
        action
            .events
            .iter()
            .any(|event| event["kind"] == "進化" && event["with"] == "form"),
        "the actual evolve-deck object supplies the form"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the real evolution trigger is pending");
    };
    assert_eq!(options.len(), 1, "one printed evolution trigger is pending");
    assert_eq!(
        options[0].card, form,
        "each actual printing supplies its ability"
    );
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn finish(game: &mut Game, play: CardPlay, steel: bool) {
    if let CardPlay::Awaiting(question) = play {
        let InputDomain::TokenCapacity { choices } = &question.domain else {
            panic!("only mixed field capacity may require a choice");
        };
        let name = if steel {
            "スティールナイト"
        } else {
            "ナイト"
        };
        let handle =
            u64::try_from(choices.iter().position(|names| names == &[name]).unwrap()).unwrap();
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::TokenCapacity(handle),
            )
            .unwrap(),
        );
    } else {
        resolved(play);
    }
}

fn assert_arrivals(game: &Game, occupied: usize, steel: bool, expected: usize) {
    let view = game.projection(View::Referee).unwrap();
    let arrivals = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| id.as_str().unwrap().starts_with("new-"))
        .map(|id| &view["objects"][id.as_str().unwrap()])
        .collect::<Vec<_>>();
    assert_eq!(
        arrivals.len(),
        expected,
        "only the free slots receive the simultaneous batch"
    );
    for (number, count, stat) in [
        (
            "BP17-T12",
            usize::from(occupied == 0 || (occupied == 3 && steel)),
            2_u8,
        ),
        (
            "BP17-T11",
            usize::from(occupied == 0 || (occupied == 3 && !steel)),
            1_u8,
        ),
    ] {
        let matching = arrivals
            .iter()
            .filter(|card| card["card"] == number)
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            count,
            "Q66 permits either printed name in the last slot"
        );
        for card in matching {
            assert_eq!(
                card["power"], stat,
                "the complete token has its printed power"
            );
            assert_eq!(
                card["hp"], stat,
                "the complete token has its printed health"
            );
        }
    }
    assert_eq!(
        view["objects"]["source"]["power"], 4_u8,
        "the real evolved mother has four power"
    );
    assert_eq!(
        view["objects"]["source"]["hp"], 4_u8,
        "the real evolved mother has four health"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 9_u8,
        "the formal evolution pays one PP"
    );
    assert_eq!(view["P1"]["ex"], json!([]), "no token is placed in EX");
    assert_eq!(
        view["P2"]["field"],
        json!([]),
        "the opponent receives no token"
    );
}

#[test]
fn pr_006_printings_evolve_from_real_floral_fencer_and_choose_either_last_slot_token() {
    for form in ["PR-006", "PR-103", "SD02-004"] {
        for (occupied, steel) in [(0_usize, true), (3, true), (3, false), (4, true)] {
            let mut game = game(&position(form, occupied));
            let play = evolve_and_trigger(&mut game, form);
            finish(&mut game, play, steel);
            assert_arrivals(&game, occupied, steel, 2.min(4 - occupied));
        }
    }
}
