#![expect(
    clippy::panic,
    reason = "Unexpected card decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::{Answer, CardPlay, InputDomain, ResumeStage};
use sve_engine::game::{Game, View};
use sve_engine::ir::PlayerId;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, life: u8) -> Value {
    let mut initial = setup("P1", "P1", [life, 20]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP01-037"},
        {"id":"d2","card":"BP01-037"},
        {"id":"d3","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial
}

fn make_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp02-title",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::TokenCapacity { .. }
            | InputDomain::MealPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Modes { .. } => panic!("unexpected card decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter_and_trigger(game: &mut Game, card: &str) {
    let play = game.play_internal_card("source", actor()).unwrap();
    let CardPlay::Entered(entry) = finish(game, play) else {
        panic!("the follower enters the field");
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("the entered card has a pending Fanfare");
    };
    assert_eq!(options.len(), 1, "this card has one pending Fanfare");
    assert_eq!(
        options[0].card, card,
        "the pending Fanfare belongs to the card that entered"
    );
    let pending_play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    assert!(
        matches!(finish(game, pending_play), CardPlay::Resolved(_)),
        "the selected Fanfare resolves"
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn title_anchor_is_in_the_snapshot_and_rules_bound_selection() {
    assert!(inputs().0.lines().any(|line| {
        serde_json::from_str::<Value>(line).is_ok_and(|card| card["number"] == "ECP02-001")
    }));
    let catalog = serde_json::to_value(&*selection_with_rules().prototypes).unwrap();
    assert!(
        catalog["rule_bindings"]["titles"]["idolmaster_cinderella_girls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "opening_ex_resource")
    );
}

#[test]
fn cp02_018_and_printed_alias_heal_draw_and_have_ward() {
    for card in ["CP02-018", "CP02-SL04"] {
        let mut game = make_game(&position(card, 15));
        enter_and_trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 18_u8);
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["deck"], json!(["d3"]));

        let mut ward_position = position("BP01-037", 20);
        ward_position["players"]["P1"]["zones"]["hand"] = json!([]);
        ward_position["players"]["P1"]["zones"]["field"] =
            json!([{"id":"attacker","card":"BP01-037"}]);
        ward_position["players"]["P2"]["zones"]["field"] =
            json!([{"id":"ward","card":card,"state":{"acted":true}}]);
        assert_eq!(
            make_game(&ward_position)
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "cannot-attack"
        );
    }
}

#[test]
fn cp02_076_and_printed_alias_damage_own_leader_and_draw() {
    for card in ["CP02-076", "CP02-P61"] {
        let mut game = make_game(&position(card, 20));
        enter_and_trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 18_u8);
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["deck"], json!(["d3"]));
    }
}

#[test]
fn cp02_077_and_printed_alias_damage_selected_enemy_follower() {
    for card in ["CP02-077", "CP02-P62"] {
        let mut initial = position(card, 20);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = make_game(&initial);
        enter_and_trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    }
}

#[test]
fn cp02_087_and_printed_alias_draw_one() {
    for card in ["CP02-087", "CP02-SL17"] {
        let mut game = make_game(&position(card, 20));
        enter_and_trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
        assert_eq!(state["P1"]["deck"], json!(["d2", "d3"]));
    }
}
