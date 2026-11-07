use super::*;
use core::iter::once;
use sve_engine::game::internal::Fact;

fn selection(steps: &Value) -> (String, String, Value) {
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![json!({"spell":{"do":steps}})],
    );
    let (member, template) = author(
        "member",
        "22222222222222222222222222222222",
        "フォロワー",
        vec![],
    );
    (
        format!("{creator}\n{member}\n"),
        json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string(),
        json!({"documents":[],"authors":[source,template]}),
    )
}

fn board(steps: &Value, hand: u8, deck: u8) -> Game {
    board_for(steps, hand, deck, 1)
}

#[expect(
    clippy::indexing_slicing,
    reason = "Fixed synthetic positions must fail when their shape changes."
)]
fn board_for(steps: &Value, hand: u8, deck: u8, seat: u32) -> Game {
    let (snapshot, keywords, input) = selection(steps);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut setup = initial(0);
    let actor = if seat == 1 { "P1" } else { "P2" };
    setup["turn"]["active"] = json!(actor);
    setup["players"]["P1"]["zones"]["hand"] = json!([]);
    setup["players"][actor]["zones"]["hand"] = json!(
        once(json!({"id":"creator","card":"creator"}))
            .chain((0..hand).map(|n| json!({"id":format!("h-{n}"),"card":"member"})))
            .collect::<Vec<_>>()
    );
    setup["players"][actor]["zones"]["deck"] = json!(
        (0..deck)
            .map(|n| json!({"id":format!("d-{n}"),"card":"member"}))
            .collect::<Vec<_>>()
    );
    Game::from_selected_effects(
        selected,
        &setup,
        &Value::Null,
        &Value::Null,
        "effect-victory",
    )
    .unwrap()
}

#[test]
fn arbitrary_discard_reads_actual_count_and_keeps_the_question_private() {
    for count in [0_usize, 1, 3] {
        let mut game = board(
            &json!([
                {"discard":{"any":true,"who":"self","result":"discarded"}},
                {"draw":{"n":{"plus":[{"count_of":"discarded"},1_u8]}}}
            ]),
            3,
            6,
        );
        let CardPlay::Awaiting(question) = game
            .play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("discard choice")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("discard domain")
        };
        assert_eq!(domain.cardinality.minimum.0, 0);
        assert_eq!(domain.cardinality.maximum.unwrap().0, 3);
        assert!(game.internal_decision(View::P2).is_none());
        assert!(game.projection(View::P2).unwrap()["objects"]["h-0"].is_null());
        let before = format!("{game:?}");
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 0]))
            .unwrap_err();
        assert_eq!(format!("{game:?}"), before);
        let answer = (0..u64::try_from(count).unwrap()).collect();
        let result = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(answer))
            .unwrap();
        assert!(matches!(result, CardPlay::Resolved(_)));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 4);
        assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 5 - count);
        assert_eq!(view["P1"]["cemetery"].as_array().unwrap().len(), count + 1);
    }
}

#[test]
fn victory_precedes_empty_deck_loss_and_stops_the_remaining_body() {
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut game = board_for(
            &json!([
                {"draw":{"n":1_u8}},
                {"win":{"who":"self"}},
                {"life":{"who":"self","delta":-99_i64}}
            ]),
            0,
            0,
            seat,
        );
        let result = game
            .play_internal_card("creator", PlayerId { seat })
            .unwrap();
        let CardPlay::GameEnded(terminal) = result else {
            panic!("effect victory terminates the body")
        };
        let progress = terminal.progress.unwrap();
        assert!(!progress.complete);
        assert!(!progress.facts.iter().any(|fact| matches!(
            fact,
            Fact::CardResolved { .. } | Fact::AbilityResolved { .. }
        )));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["game"]["ended"], true);
        assert_eq!(view["game"]["winner"], actor);
        assert_eq!(view["game"]["by"], "1.2.4");
        assert_eq!(view[actor]["leader"]["life"], 20_u8);
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert!(game.internal_decision(View::P1).is_none());
        game.concede_internal(PlayerId { seat: 2 }).unwrap_err();
    }
}

#[test]
fn victory_and_any_discard_keep_unsupported_author_variants_closed() {
    for step in [
        json!({"win":{"who":"opp"}}),
        json!({"win":{"who":"self","result":"won"}}),
        json!({"discard":{"any":true,"who":"opp"}}),
        json!({"discard":{"any":true,"n":1_u8}}),
        json!({"discard":{"any":true,"random":true}}),
    ] {
        let (snapshot, keywords, input) = selection(&json!([step]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
    let (snapshot, keywords, input) = selection(&json!([
        {"discard":{"any":true,"result":"discarded"}},
        {"move":{"ref":{"in":{"objects_of":"discarded","view":"before"}},"to":"deck","position":"bottom","order":"chooser"}}
    ]));
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
}
