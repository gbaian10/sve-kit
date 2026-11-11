#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Fanfare scenario."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };
const DISTINCT: [&str; 9] = [
    "BP19-P31", "BP01-176", "BP01-170", "BP01-171", "BP01-174", "BP01-177", "BP01-179",
    "BP19-115", "BP19-119",
];

fn fanfare(board: &mut Game) -> CardPlay {
    assert!(
        matches!(
            board.play_internal_card("source", PLAYER).unwrap(),
            CardPlay::Entered(_)
        ),
        "the main-deck follower enters before its Fanfare"
    );
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected Zerael's Fanfare");
    };
    assert_eq!(options.len(), 1, "only the printed Fanfare is pending");
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
}

fn position(card: &str, revealed: &[&str], with_tail: bool) -> (Game, Vec<String>) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"advance","card":"BP19-P32"}]);
    let mut deck = revealed
        .iter()
        .enumerate()
        .map(|(index, number)| json!({"id":format!("d{index}"),"card":number}))
        .collect::<Vec<_>>();
    let shuffled = (0..deck.len())
        .rev()
        .map(|index| format!("d{index}"))
        .collect::<Vec<_>>();
    let mut final_deck = shuffled.clone();
    if with_tail {
        deck.push(json!({"id":"tail","card":"BP01-176"}));
        final_deck.insert(0, "tail".into());
    }
    initial["players"]["P1"]["zones"]["deck"] = json!(deck);
    let random = json!({"shuffles":[{"player":"P1","result":shuffled}]});
    let board = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &random,
        "zerael-current",
    )
    .unwrap();
    (board, final_deck)
}

#[test]
fn zerael_both_printings_shuffle_the_revealed_prefix_and_offer_the_advance() {
    for (card, accept) in [("BP19-113", true), ("BP19-P31", false)] {
        let (mut board, final_deck) = position(card, &DISTINCT, true);
        let CardPlay::Awaiting(question) = fanfare(&mut board) else {
            panic!("nine distinct revealed cards allow the optional Advance");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("expected the private Advance choice");
        };
        assert_eq!(domain.cardinality.minimum, Nat(0));
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("BP19-P32"));
        assert!(board.internal_decision(View::P2).is_none());
        let before_entry = board.projection(View::Referee).unwrap();
        assert_eq!(before_entry["P1"]["deck"], json!(final_deck));
        assert_eq!(before_entry["P1"]["cemetery"], json!(["source"]));
        assert_eq!(before_entry["P1"]["field"], json!([]));

        let opponent = board.projection(View::P2).unwrap();
        for id in &final_deck {
            assert!(opponent["objects"][id].is_null(), "shuffled positions stay private");
        }
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(if accept { vec![domain.members[0].handle] } else { vec![] }),
            )
            .unwrap();
        if accept {
            let CardPlay::Awaiting(ward) = play else {
                panic!("the chosen Advance enters through its Ward decision");
            };
            assert!(matches!(ward.domain, InputDomain::WardEntry { .. }));
            board
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap();
        } else {
            resolved(play);
        }
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            if accept { json!(["advance"]) } else { json!([]) }
        );
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    }
}

#[test]
fn zerael_keeps_its_source_with_empty_short_or_duplicate_reveals() {
    let mut duplicate = DISTINCT;
    duplicate[8] = "BP19-113";
    for (cards, with_tail) in [(&[][..], false), (&DISTINCT[..8], false), (&duplicate[..], true)] {
        let (mut board, final_deck) = position("BP19-113", cards, with_tail);
        resolved(fanfare(&mut board));
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(final_deck));
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["evolve_deck"], json!(["advance"]));
    }
}
