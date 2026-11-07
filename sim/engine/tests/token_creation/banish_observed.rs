use super::*;

fn inputs(steps: &Value) -> (String, String, Value) {
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

fn steps() -> Value {
    json!([
        {"look":{"bind":"seen","top":{"who":"self","n":5_u8}}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2_u8,"may":true}},
        {"banish":{"ref":"picked","result":"banished"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"banished","view":"before"}}},
            "to":"deck","position":"bottom","order":"chooser"}}
    ])
}

#[test]
fn looked_cards_are_banished_in_one_batch_and_only_actual_moves_leave_the_remainder() {
    for count in [0_usize, 1, 2] {
        let (snapshot, keywords, input) = inputs(&steps());
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let mut initial = initial(0);
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0..6_u8)
                .map(|n| json!({"id":format!("d-{n}"),"card":"member"}))
                .collect::<Vec<_>>()
        );
        let mut board = Game::from_selected_effects(
            selected,
            &initial,
            &Value::Null,
            &Value::Null,
            "observed-banish",
        )
        .unwrap();
        let CardPlay::Awaiting(question) = board
            .play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("private choice");
        };
        assert!(board.internal_decision(View::P2).is_none());
        assert!(
            !board
                .projection(View::P2)
                .unwrap()
                .to_string()
                .contains("d-0")
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("private looked-card choice");
        };
        assert_eq!(domain.members.len(), 5);
        let handles = domain
            .members
            .iter()
            .take(count)
            .map(|m| m.handle)
            .collect();
        let CardPlay::Awaiting(order) = board
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap()
        else {
            panic!("remainder order");
        };
        let InputDomain::Order(cards) = &order.domain else {
            panic!("remainder order domain");
        };
        assert_eq!(cards.len(), 5 - count);
        assert!(board.internal_decision(View::P2).is_none());
        let play = board
            .answer_internal_card(
                order.actor,
                order.token,
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect()),
            )
            .unwrap();
        assert!(matches!(play, CardPlay::Resolved(_)));
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["banish"],
            json!((0..count).map(|n| format!("d-{n}")).collect::<Vec<_>>())
        );
        let mut expected = vec!["d-5".to_owned()];
        expected.extend((count..5).rev().map(|n| format!("d-{n}")));
        assert_eq!(view["P1"]["deck"], json!(expected));
        assert_eq!(view["objects"]["d-5"]["generation"], 0_u8);
    }
}

#[test]
fn observed_banish_does_not_authorize_unobserved_decks_or_result_following() {
    let mut variants = Vec::new();
    let mut opponent = steps();
    opponent[0]["look"]["top"]["who"] = json!("opp");
    variants.push(opponent);
    let mut unobserved = steps();
    unobserved[1]["choose"]["from"] = json!({"who":"self","zone":"deck"});
    variants.push(unobserved);
    let mut source_result = steps();
    source_result[2]["banish"]["ref"] = json!("this");
    variants.push(source_result);
    let mut following = steps();
    following[3] = json!({"move":{"ref":{"follow":"banished"},"to":"ex"}});
    variants.push(following);
    let mut moved_input = steps();
    moved_input[3]["move"]["ref"] = json!({"in":{"objects_of":"banished","view":"before"}});
    variants.push(moved_input);
    for (index, body) in variants.into_iter().enumerate() {
        let (snapshot, keywords, input) = inputs(&body);
        assert!(
            Catalog::from_current_selection(&snapshot, &keywords, &input).is_err(),
            "variant {index} grants no extra private read or generation following"
        );
    }
}
