use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "banish-zone-observer",
    )
    .unwrap()
}

fn cards(prefix: &str, card: &str, count: u8) -> Value {
    json!(
        (0..count)
            .map(|index| json!({"id":format!("{prefix}{index}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

/// Prefers an opposing member (damage targets), otherwise the first one (hand or evolve picks).
fn settle(game: &mut Game, mut step: Option<CardPlay>) {
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .find(|m| m.player.seat == 2)
                        .or_else(|| d.members.first())
                        .map(|m| m.handle)
                        .into_iter()
                        .collect(),
                ),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { .. } => Answer::Quick(None),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                // BP20-041's on-evolve head cost (another Idol to the cemetery) is declined.
                InputDomain::OptionalCost => Answer::OptionalCost(false),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    settle(game, Some(step));
}

#[test]
fn bp22_037_damages_an_enemy_follower_when_its_controller_banishes_a_card() {
    // BP17-051 draws, then banishes the drawn card from hand during P1's turn.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-037"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"banisher","card":"BP17-051"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}]);
    let mut game = game(&initial);
    play(&mut game, "banisher");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["banish"], json!(["drawn"]));
    assert_eq!(view["objects"]["enemy"]["damage"], 1_u8);
}

#[test]
fn bp22_037_fanfare_evolves_the_next_witch_follower_only_from_ten_banished() {
    for (banished, evolves) in [(9_u8, false), (10, true)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":"BP22-037"},
            {"id":"axia","card":"BP20-040"}
        ]);
        initial["players"]["P1"]["zones"]["banish"] = cards("gone", "BP01-037", banished);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"axia-form","card":"BP20-041"},
            {"id":"chrono-form","card":"BP22-038"}
        ]);
        let mut game = game(&initial);
        play(&mut game, "source");
        play(&mut game, "axia");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["evolve_deck"],
            if evolves {
                json!(["chrono-form"])
            } else {
                json!(["axia-form", "chrono-form"])
            },
            "{banished} banished"
        );
    }
}

#[test]
fn bp22_038_recovers_two_banished_followers_for_zero_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-038"}]);
    initial["players"]["P1"]["zones"]["banish"] = json!([
        {"id":"erina","card":"BP22-046"}, {"id":"cat","card":"BP22-042"}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );

    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Chronowitch's evolve trigger");
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("Chronowitch must declare banish targets");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("Chronowitch's banish targets");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "both banished Witch followers qualify"
    );
    let selected = domain.members.iter().map(|member| member.handle).collect();
    let next = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(selected))
        .unwrap();
    resolved(next);
    settle(&mut game, None);

    let after_return = game.projection(View::Referee).unwrap();
    assert_eq!(after_return["P1"]["banish"], json!([]));
    let ex = after_return["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 2, "both targets move to EX");
    assert_eq!(after_return["P1"]["pp"]["current"], 0_u8);
    for id in ["erina", "cat"] {
        assert!(ex.contains(&json!(id)), "{id} moved to EX");
        play(&mut game, id);
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"].as_array().unwrap().contains(&json!(id)),
            "{id} enters from EX"
        );
        assert_eq!(view["P1"]["pp"]["current"], 0_u8, "{id} costs zero PP");
    }
}
