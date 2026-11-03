#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn in_hand(card: &str, library: &[(&str, &str)]) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = Value::Array(
        library
            .iter()
            .map(|(id, number)| json!({"id":id,"card":number}))
            .collect(),
    );
    initial
}

// Collaboration cards need the registered title rules.
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "result-card-condition",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(1).map(|card| card.handle).collect(),
            )),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(card: &str, library: &[(&str, &str)]) -> Game {
    let mut game = game(&in_hand(card, library));
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, step);
    triggers(&mut game);
    game
}

fn triggers(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
    }
}

/// Evolves the printed base on the field into `form`, then resolves its evolve trigger.
fn evolve(base: &str, form: &str, library: &[(&str, &str)]) -> Game {
    let mut initial = in_hand(base, library);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    let outcome = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap()
        .outcome;
    assert_eq!(outcome, "resolved", "the base pays its printed evolution cost");
    triggers(&mut game);
    game
}

fn hand(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()["P1"]["hand"].clone()
}

#[test]
fn bp17_045_draws_only_when_a_milled_card_is_not_witch() {
    let witch_only = play(
        "BP17-045",
        &[("w1", "CSD02a-004"), ("w2", "BP17-045"), ("rest", "BP01-171")],
    );
    let state = witch_only.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["w1", "w2"]));
    assert_eq!(hand(&witch_only), json!([]), "two Witch cards draw nothing");

    let mixed = play(
        "BP17-045",
        &[("w1", "CSD02a-004"), ("neutral", "BP01-171"), ("rest", "BP03-066")],
    );
    assert_eq!(hand(&mixed), json!(["rest"]), "a neutral card draws one");
}

#[test]
fn cp02_015_heals_only_after_revealing_a_cute_card() {
    let cute = evolve("CP02-014", "CP02-015", &[("top", "CSD02a-004"), ("rest", "BP01-171")]);
    let state = cute.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["top"]));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8, "the revealed cute card heals two");

    let cool = evolve("CP02-014", "CP02-015", &[("top", "CP02-006"), ("rest", "BP01-171")]);
    let declined = cool.projection(View::Referee).unwrap();
    assert_eq!(declined["P1"]["hand"], json!(["top"]));
    assert_eq!(declined["P1"]["leader"]["life"], 20_u8, "a cool card is not cute");
}

#[test]
fn ecp02_040_reads_each_milled_trait_independently() {
    let game = play(
        "ECP02-040",
        &[("cute", "CSD02a-004"), ("passion", "CP02-001"), ("neutral", "BP01-171")],
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8, "a cute card heals two");
    assert_eq!(state["objects"]["source"]["power"], 5_u8, "a passion card adds two attack");
    assert!(
        !state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃")),
        "no cool card, no assail"
    );

    let cool = play(
        "ECP02-040",
        &[("cool", "CP02-006"), ("n1", "BP01-171"), ("n2", "BP01-171")],
    );
    let cooled = cool.projection(View::Referee).unwrap();
    assert_eq!(cooled["P1"]["leader"]["life"], 20_u8, "no cute card");
    assert_eq!(cooled["objects"]["source"]["power"], 3_u8, "no passion card");
    assert!(
        cooled["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃")),
        "a cool card grants assail"
    );
}
