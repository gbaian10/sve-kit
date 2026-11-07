#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The scenarios answer only the questions printed on these two cards."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

const JUNGLE: &[&str] = &[
    "BP22-R24",
    "BP22-SL36",
    "ECP01-049",
    "ECP01-SL24",
    "ECP01-SP24",
    "ECP01-SSP24",
];
fn initial(number: &str) -> Value {
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3,"P2":3}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":10},"pp":{"current":10,"max":10},"ep":0,"sep":0,"zones":{
            "hand":[{"id":"source","card":number}],
            "deck":[{"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"}]}},
        "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,"zones":{}}
    }})
}

fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed decision")
    };
    question
}

fn enter(board: &mut Game) {
    let mut play = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "Ward posture is the only entry decision"
        );
        play = board
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the printed follower must enter before its trigger"
    );
}

fn pending(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

fn pay_uma(board: &mut Game, mut play: CardPlay, fuel: &str) -> CardPlay {
    let mut paid = false;
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.members.len(), 1, "only the Uma card is payable");
                assert_eq!(
                    domain.members[0].card.as_deref(),
                    Some(fuel),
                    "payment must use the selected Uma card"
                );
                assert!(
                    board.internal_decision(View::P2).is_none(),
                    "hand payment remains private"
                );
                paid = true;
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(vec![domain.members[0].handle])
                } else {
                    Answer::CostMaterials(vec![domain.members[0].handle])
                }
            }
            _ => panic!("Uma discard payment"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        paid,
        "the Fanfare must actually discard its required material"
    );
    play
}

#[test]
fn jungle_printings_replace_one_draw_with_two_from_the_discarded_original_cost() {
    for number in JUNGLE {
        for (fuel, effective_cost, draws) in [("ECP01-055", 9_u8, 1_usize), ("ECP01-034", 1, 2)] {
            let mut initial = initial(number);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number},{"id":"fuel","card":fuel,"state":{"cost":effective_cost}},{"id":"unrelated","card":"BP01-171"}]);
            let mut board = game(&initial);
            enter(&mut board);
            let play = pending(&mut board, number, 2);
            resolved(pay_uma(&mut board, play, fuel));
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
            let mut hand = vec!["unrelated".to_owned()];
            hand.extend((0..draws).map(|i| format!("d{i}")));
            assert_eq!(state["P1"]["hand"], json!(hand));
            assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        }
    }
}

#[test]
fn jungle_counts_only_own_uma_cards_and_checks_each_damage_clause_separately() {
    for count in [2_usize, 3, 4, 5] {
        let mut initial = initial("BP22-R24");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![json!({"id":"source","card":"BP22-R24"})];
        field.extend((1..count).map(|i| json!({"id":format!("ally-{i}"),"card":"ECP01-028"})));
        if count < 5 {
            field.push(json!({"id":"other","card":"BP01-171"}));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut board = game(&initial);
        let before = board.projection(View::Referee).unwrap();
        let ability = selection_with_rules().current["BP22-R24"]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated && a.source.line == 3)
            .unwrap()
            .node
            .id;
        let question = waiting(
            board
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("mandatory opposing follower")
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        let damage = if count >= 3 { 2_i64 } else { 0 };
        assert_eq!(
            state["objects"]["enemy"]["hp"].as_i64().unwrap(),
            before["objects"]["enemy"]["hp"].as_i64().unwrap() - damage
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if count == 5 { 18_u8 } else { 20 }
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}
