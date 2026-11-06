#![expect(
    clippy::panic,
    reason = "These printed abilities must expose their target and search questions."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn initial() -> Value {
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3,"P2":3}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":3,"sep":0,"zones":{
            "evolve_deck":[{"id":"first","card":"BP04-043","face_up":true},{"id":"second","card":"BP03-021","face_up":true},{"id":"down","card":"BP03-021","face_up":false}],
            "deck":[{"id":"draw","card":"BP01-171"},{"id":"padding","card":"BP01-171"}]}},
        "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,"zones":{}}
    }})
}

fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed choice")
    };
    question
}

fn pending(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    board
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

#[test]
fn europa_fanfare_requires_a_face_up_europa_and_buffs_only_itself() {
    for available in [false, true] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP04-042"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"][0]["face_up"] = json!(available);
        let mut board = game(&initial);
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let before = board.projection(View::Referee).unwrap();
        let play = pending(&mut board, "BP04-042");
        if available {
            let question = waiting(play);
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("Europa target")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("BP04-043"));
            resolved(
                board
                    .answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(vec![domain.members[0].handle]),
                    )
                    .unwrap(),
            );
        } else {
            assert!(
                matches!(play, CardPlay::Cancelled),
                "Q585: the entire trigger needs its formal target"
            );
        }
        let state = board.projection(View::Referee).unwrap();
        let increase = i64::from(available);
        assert_eq!(
            state["objects"]["source"]["power"].as_i64().unwrap(),
            before["objects"]["source"]["power"].as_i64().unwrap() + increase
        );
        assert_eq!(
            state["objects"]["source"]["hp"].as_i64().unwrap(),
            before["objects"]["source"]["hp"].as_i64().unwrap() + increase
        );
        assert_eq!(state["objects"]["first"]["face_up"], false);
        assert_eq!(state["objects"]["second"]["face_up"], true);
    }
}

#[test]
fn reina_evolution_searches_a_follower_only_after_a_legal_flip() {
    for available in [false, true] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP08-110"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-111","face_up":false},{"id":"old","card":"BP03-021","face_up":available}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"spell","card":"SD03-005"},{"id":"found","card":"BP01-171"}]);
        let mut board = game(&initial);
        let action = board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
            )
            .unwrap();
        assert_eq!(action.outcome, "resolved");
        let play = pending(&mut board, "BP08-111");
        if !available {
            assert!(
                matches!(play, CardPlay::Cancelled),
                "Q1143: no flip target means no search"
            );
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["hand"],
                json!([])
            );
            continue;
        }
        let question = waiting(play);
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("evolve follower")
        };
        assert_eq!(domain.members.len(), 1);
        let search = waiting(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        let InputDomain::Search(search_domain) = &search.domain else {
            panic!("follower search")
        };
        assert_eq!(search_domain.cards.len(), 1);
        assert!(board.internal_decision(View::P2).is_none());
        resolved(
            board
                .answer_internal_card(
                    search.actor,
                    search.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![search_domain.cards[0].handle])),
                )
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["old"]["face_up"], false);
        assert_eq!(state["P1"]["hand"], json!(["found"]));
        assert_eq!(state["P1"]["deck"], json!(["spell"]));
    }
}

#[test]
fn sowing_printings_draw_before_flipping_zero_or_many_and_gain_ep_above_three() {
    for number in ["BP18-P50", "BP18-T01"] {
        for count in [0, 2] {
            let mut initial = initial();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["evolve_deck"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"drive","card":"CSD03a-017","face_up":true}),
                    json!({"id":"advance","card":"BP22-R02","face_up":true}),
                ]);
            let mut board = game(&initial);
            let question = waiting(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["draw"]),
                "draw precedes the resolution choice"
            );
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("any face-up forms")
            };
            assert_eq!(domain.cardinality.minimum, Nat(0));
            assert_eq!(
                domain.members.len(),
                2,
                "Q2375: exclude Drive Point, Advance and face-down cards"
            );
            let handles = domain
                .members
                .iter()
                .take(count)
                .map(|member| member.handle)
                .collect();
            resolved(
                board
                    .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                    .unwrap(),
            );
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["ep"], 4_u8,
                "Q2374: first player may exceed three EP"
            );
            assert_eq!(state["objects"]["first"]["face_up"], count == 0);
            assert_eq!(state["objects"]["second"]["face_up"], count == 0);
            assert_eq!(state["objects"]["drive"]["face_up"], true);
            assert_eq!(state["objects"]["advance"]["face_up"], true);
        }
    }
}
