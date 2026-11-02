#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these real exchange scenarios."
)]
use super::*;

// Blank companions isolate exchange payment from unrelated event subscriptions.
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| {
            author["input"]["card_no"].as_str().is_some_and(|number| ["ECP01-036", "BP14-119", "CP02-038", "CP02-T01", "BP03-T02"].contains(&number))
        });
        for (number, cost, traits) in [
            ("exchange-high", "7", vec!["ウマ娘"]),
            ("exchange-low", "1", vec!["ウマ娘"]),
            ("exchange-goblin", "1", vec!["ゴブリン"]),
            ("exchange-idol", "1", vec!["デレマス"]),
            ("exchange-dummy", "1", vec![]),
        ] {
            if !snapshot.ends_with('\n') { snapshot.push('\n'); }
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":cost,"power":"1","hp":"20","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn exchange_game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "hand-exchange",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "exchange-high",
        "exchange-low",
        "exchange-goblin",
        "exchange-idol",
        "exchange-dummy",
    ] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

fn position(number: &str, hand: Value, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P1"]["zones"]["hand"] = hand;
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"source","card":number}));
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw0","card":"exchange-dummy"},{"id":"draw1","card":"exchange-dummy"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"exchange-dummy","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, accepted: bool, pp: u8) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accepted),
            InputDomain::Targets(_) => Answer::Targets(vec![0]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    usize::try_from(domain.cardinality.minimum.0).unwrap(),
                    "only matching other hand cards are eligible"
                );
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "hand materials remain private"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    pp,
                    "all choices precede payment"
                );
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            _ => panic!("unexpected exchange question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

#[test]
fn ecp01_036_exchange_checks_the_paid_cards_original_cost_for_every_printing() {
    for number in ["ECP01-036", "ECP01-P20"] {
        for (accepted, card, effective, draw) in [
            (true, "exchange-high", 1_u8, true),
            (true, "exchange-low", 9, false),
            (false, "exchange-high", 1, false),
        ] {
            let initial = position(
                number,
                json!([{"id":"fuel","card":card,"state":{"cost":effective}}]),
                3,
            );
            let mut game = exchange_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            resolved(finish(&mut game, play, accepted, 3));
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"],
                if accepted { 2_u8 } else { 0_u8 }
            );
            assert_eq!(view["objects"]["enemy"]["damage"], 5_u8);
            assert_eq!(
                view["P1"]["cemetery"],
                if accepted {
                    json!(["fuel", "source"])
                } else {
                    json!(["source"])
                }
            );
            assert_eq!(
                view["P1"]["deck"].as_array().unwrap().len(),
                if draw { 1 } else { 2 }
            );
        }
    }
}

#[test]
fn bp14_119_discards_a_goblin_and_plays_for_zero_pp() {
    let initial = position(
        "BP14-119",
        json!([{"id":"fuel","card":"exchange-goblin"},{"id":"wrong","card":"exchange-dummy"}]),
        0,
    );
    let mut game = exchange_game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, true, 0));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 0_u8);
    assert_eq!(view["P1"]["hand"], json!(["wrong"]));
    assert_eq!(view["P1"]["cemetery"], json!(["fuel", "source"]));
    assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn cp02_038_discards_three_other_cards_before_entry_then_resolves_fanfare_for_every_printing() {
    for number in ["CP02-038", "CP02-P29"] {
        let initial = position(
            number,
            json!([
                {"id":"fuel0","card":"exchange-idol"},{"id":"fuel1","card":"exchange-idol"},{"id":"fuel2","card":"exchange-idol"}
            ]),
            3,
        );
        let mut game = exchange_game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(
            finish(&mut game, play, true, 3),
            CardPlay::Entered(_)
        ));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["fuel0", "fuel1", "fuel2"]));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("real Fanfare")
        };
        let handle = options
            .iter()
            .find(|option| option.card == number)
            .unwrap()
            .handle;
        let trigger = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap();
        resolved(finish(&mut game, trigger, true, 0));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(after["P1"]["hand"], json!(["draw0", "draw1"]));
    }
}
