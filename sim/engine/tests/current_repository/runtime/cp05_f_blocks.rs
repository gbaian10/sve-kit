#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]

use super::*;

const COMPANIONS: [(&str, &str, &str, &str); 3] = [
    ("eligible", "Look candidate", "フォロワー", "5"),
    ("wrong-cost", "Original cost four", "フォロワー", "4"),
    ("wrong-kind", "Granblue spell", "スペル", "5"),
];

fn fixture_selection() -> &'static SelectedEffects {
    static SELECTION: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTION.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost) in
            COMPANIONS
                .into_iter()
                .chain([("vira", "狂恋の華鎧・ヴィーラ", "フォロワー", "3")])
        {
            let follower = kind == "フォロワー";
            append(
                &mut snapshot,
                &mut input,
                number,
                &json!({
                    "name":name,"card_class":"ビショップ","card_type":kind,"cost":cost,
                    "power":if follower {"2"} else {"-"},"hp":if follower {"3"} else {"-"},
                    "traits":["グラブル"],"text":null,"sections":[]
                }),
            );
        }
        repository::select(&snapshot, &keywords, &input).unwrap()
    })
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture_selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-bishop-blocks",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in COMPANIONS
        .map(|(number, ..)| number)
        .into_iter()
        .chain(["vira"])
    {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn answer(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

fn choose(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a selection")
    };
    question
}

#[test]
fn funf_excludes_its_reprint_keeps_look_private_and_orders_the_remainder() {
    let card = "CP05-090";
    let mut initial = position_with_source(card, "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"CP05-T15"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same","card":"CP05-SL22"},
        {"id":"wrong-cost","card":"wrong-cost"},
        {"id":"wrong-trait","card":"BP01-037"},
        {"id":"eligible","card":"eligible"},
        {"id":"wrong-kind","card":"wrong-kind"},
        {"id":"unseen","card":"BP01-037"}
    ]);
    let mut game = game(&initial);

    let action = game
        .activate_internal_card("material", activated("CP05-T15"), actor())
        .unwrap();
    let question = choose(action);
    let InputDomain::Targets(candidates) = &question.domain else {
        panic!("material target")
    };
    resolved(answer(
        &mut game,
        &question,
        Answer::Targets(vec![candidates.members[0].handle]),
    ));
    game.resume_internal_card().unwrap();

    let question = choose(resolve_pending_card(&mut game, card));
    let ordinary = answer(&mut game, &question, Answer::Targets(vec![]));
    resolved(ordinary);
    game.resume_internal_card().unwrap();
    let look = choose(resolve_pending_card(&mut game, card));
    let InputDomain::Targets(candidates) = &look.domain else {
        panic!("Look selection")
    };
    assert!(game.internal_decision(View::P2).is_none());
    assert_eq!(candidates.members.len(), 1);
    assert_eq!(candidates.members[0].card.as_deref(), Some("eligible"));
    let picked = answer(
        &mut game,
        &look,
        Answer::Targets(vec![candidates.members[0].handle]),
    );
    let order = choose(accept(&mut game, picked));
    let InputDomain::Order(cards) = &order.domain else {
        panic!("Look remainder order")
    };
    assert_eq!(cards.len(), 4);
    let remainder = cards.iter().rev().map(|card| card.handle).collect();
    resolved(answer(&mut game, &order, Answer::Order(remainder)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "eligible"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "wrong-kind", "wrong-trait", "wrong-cost", "same"])
    );
    assert_eq!(state["P1"]["pp"]["current"], 8);
}

#[test]
fn affection_oath_only_searches_after_the_private_discard_succeeds() {
    for discard in [false, true] {
        let mut initial = position_with_source("CP05-P80", "hand");
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":"CP05-P80"},
            {"id":"offering","card":"CP05-090"},
            {"id":"wrong-cost","card":"wrong-cost"},
            {"id":"wrong-trait","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"vira"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":10,"max_hp":10}}
        ]);
        let mut game = game(&initial);

        let target = choose(game.play_internal_card("source", actor()).unwrap());
        let InputDomain::Targets(candidates) = &target.domain else {
            panic!("formal spell target")
        };
        let hand = choose(answer(
            &mut game,
            &target,
            Answer::Targets(vec![candidates.members[0].handle]),
        ));
        let InputDomain::Targets(candidates) = &hand.domain else {
            panic!("private discard choice")
        };
        assert!(game.internal_decision(View::P2).is_none());
        assert_eq!(candidates.members.len(), 1);
        assert_eq!(candidates.members[0].card.as_deref(), Some("CP05-090"));
        let before_discard = game.projection(View::Referee).unwrap();
        assert_eq!(before_discard["objects"]["enemy"]["hp"], 8);
        assert_eq!(before_discard["P1"]["ex"].as_array().unwrap().len(), 1);
        assert_eq!(before_discard["P1"]["deck"], json!(["found"]));

        let chosen = if discard {
            vec![candidates.members[0].handle]
        } else {
            vec![]
        };
        let paid = answer(&mut game, &hand, Answer::Targets(chosen));
        if discard {
            let search = choose(paid);
            let InputDomain::Search(domain) = &search.domain else {
                panic!("search after discard")
            };
            assert_eq!(domain.cards.len(), 1);
            resolved(answer(
                &mut game,
                &search,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            ));
        } else {
            resolved(paid);
        }
        let state = game.projection(View::Referee).unwrap();
        let hand = state["P1"]["hand"].as_array().unwrap();
        assert_eq!(hand.contains(&json!("found")), discard);
        assert_eq!(hand.contains(&json!("offering")), !discard);
    }
}
