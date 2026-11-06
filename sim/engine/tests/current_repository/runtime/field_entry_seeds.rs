#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed entry scenarios."
)]
use super::*;

fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, kind, cost, traits) in [
            ("entry-low", "フォロワー", "2", vec!["機械", "ゴブリン"]),
            ("entry-high", "フォロワー", "3", vec!["機械", "ゴブリン"]),
            ("entry-wrong", "フォロワー", "2", vec![]),
            ("entry-amulet", "アミュレット", "3", vec![]),
            ("entry-expensive-amulet", "アミュレット", "4", vec![]),
            ("entry-spell", "スペル", "1", vec![]),
        ] {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if follower {"2"} else {"-"},"hp":if follower {"8"} else {"-"},"traits":traits,"text":Value::Null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn aliases(number: &str) -> Vec<String> {
    let id = &selected().current[number].card_id;
    selected()
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == id)
        .map(|(printing, _)| printing.clone())
        .collect()
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "field-entry-seeds",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "entry-low",
        "entry-high",
        "entry-wrong",
        "entry-amulet",
        "entry-expensive-amulet",
        "entry-spell",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    value["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..7_u8)
            .map(|i| json!({"id":format!("d{i}"),"card":"entry-spell"}))
            .collect::<Vec<_>>()
    );
    value
}
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "only the chooser sees private Look and hand candidates"
                );
                let handles = domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect();
                Answer::Targets(handles)
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected field entry choice: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has a legal declaration"
    );
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending entry ability")
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play);
    }
}
#[test]
fn bp17_098_looks_before_putting_two_eligible_hand_cards_into_the_remaining_slots() {
    for number in aliases("BP17-098") {
        let mut initial = position(&number, "hand");
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number}, {"id":"h1","card":"entry-low"},
            {"id":"high","card":"entry-high"}, {"id":"wrong","card":"entry-wrong"}
        ]);
        initial["players"]["P1"]["zones"]["deck"][0] = json!({"id":"d0","card":"entry-low"});
        initial["players"]["P1"]["zones"]["deck"][1] = json!({"id":"d1","card":number});
        initial["players"]["P1"]["zones"]["deck"][2] = json!({"id":"d2","card":"entry-high"});
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "h1", "d0"]));
        assert_eq!(state["P1"]["hand"], json!(["high", "wrong", "d2"]));
        assert!(
            state["P1"]["deck"]
                .as_array()
                .unwrap()
                .contains(&json!("d1"))
        );
        assert_eq!(state["objects"]["h1"]["keywords"], json!(["突進"]));
    }
}
#[test]
fn bp22_112_filters_hand_cost_and_trait_after_the_look_selection() {
    for number in aliases("BP22-112") {
        let mut initial = position(&number, "hand");
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number}, {"id":"high","card":"entry-high"},
            {"id":"wrong","card":"entry-wrong"}
        ]);
        initial["players"]["P1"]["zones"]["deck"][0] = json!({"id":"d0","card":"entry-low"});
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "d0"]));
        assert_eq!(state["P1"]["hand"], json!(["high", "wrong"]));
    }
}
#[test]
fn cp02_090_evolution_puts_an_amulet_within_the_printed_cost_limit_into_play() {
    for number in aliases("CP02-090") {
        let mut initial = position("CP02-089", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"high","card":"entry-expensive-amulet"}, {"id":"wrong","card":"entry-low"}, {"id":"matching","card":"entry-amulet"}
        ]);
        let mut game = game(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "matching"]));
        assert_eq!(state["P1"]["hand"], json!(["high", "wrong"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}
