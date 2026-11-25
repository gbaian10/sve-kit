#![expect(
    clippy::panic,
    reason = "A different decision invalidates the printed-card scenario."
)]

use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn answer(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

fn answer_single_target(game: &mut Game, play: CardPlay) -> CardPlay {
    let question = awaiting(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected one target");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the test position has one eligible card"
    );
    answer(
        game,
        &question,
        Answer::Targets(vec![domain.members[0].handle]),
    )
}

fn first_trigger(game: &mut Game, number: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a pending ability");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    answer(game, &question, Answer::Pending(option.handle))
}

fn fanfare(game: &mut Game, number: &str) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PLAYER).unwrap(),
            CardPlay::Entered(_)
        ),
        "the card entered before its fanfare"
    );
    first_trigger(game, number)
}

fn enemy_position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn created_in(view: &Value, zone: &str, card: &str) -> bool {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| view["objects"][id.as_str().unwrap()]["card"] == card)
}

#[test]
fn quatre_discards_itself_to_damage_a_target_and_draw() {
    let mut initial = enemy_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-038"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let choice = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &choice.domain else {
        panic!("expected Quick window");
    };
    assert_eq!(options.len(), 1);
    let play = answer(&mut game, &choice, Answer::Quick(Some(options[0].handle)));
    resolved(answer_single_target(&mut game, play));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn quatre_limit_break_damages_a_follower_and_final_limit_break_damages_every_enemy_leader() {
    let mut initial = enemy_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-038"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"CP05-T15"}]);
    let mut game = game(&initial);
    let life = game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
        .as_u64()
        .unwrap();
    let play = game
        .activate_internal_card(
            "material",
            definition("CP05-T15", AbilityKind::Activated, 0),
            PLAYER,
        )
        .unwrap();
    resolved(answer_single_target(&mut game, play));
    for _ in 0_u8..2 {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let trigger = first_trigger(&mut game, "CP05-038");
        if matches!(trigger, CardPlay::Awaiting(_)) {
            resolved(answer_single_target(&mut game, trigger));
        } else {
            resolved(trigger);
        }
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["leader"]["life"], life - 4);
}

#[test]
fn base_cagliostro_creates_both_ex_cards_at_seven_maximum_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-041"}]);
    let mut game = game(&initial);
    resolved(fanfare(&mut game, "CP05-041"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
    assert!(created_in(&state, "ex", "CP05-T08"));
    assert!(created_in(&state, "ex", "CP05-T15"));
}

#[test]
fn evolved_cagliostro_targets_and_damages_an_enemy_on_evolution() {
    let mut initial = enemy_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-041"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-042"}]);
    let mut game = game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let play = first_trigger(&mut game, "CP05-042");
    resolved(answer_single_target(&mut game, play));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8
    );
}

// The real evolved form can resolve independently of the base card's separate admission.
fn arulumaya_game(initial: &Value) -> Game {
    let (snapshot, keywords, mut input) = inputs().clone();
    let form = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|record| record["number"] == "CP05-046")
        .unwrap();
    let base = json!({
        "number":"arulumaya-base",
        "faces":[{
            "name":form["faces"][0]["name"],
            "card_class":form["faces"][0]["card_class"],
            "card_type":"フォロワー","cost":"1","power":"1","hp":"1",
            "traits":[],"text":null,"sections":[]
        }]
    });
    let snapshot = format!("{}\n{base}", snapshot.trim_end());
    let document = json!({
        "format":1_u8,"kind":"effect_set",
        "cards":{"arulumaya-base":{"status":"complete","abilities":[]}}
    });
    input["documents"]
        .as_array_mut()
        .unwrap()
        .push(json!(["arulumaya-base.yaml", document.to_string()]));
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"arulumaya-base":tree["cards"]["synthetic-evolution"]});
    let programs = lowerer().lower(&tree).unwrap();
    let mut game = game_from_selection(initial, &snapshot, &keywords, &input);
    game.load_internal_card("arulumaya-base", &programs["arulumaya-base"])
        .unwrap();
    game
}

#[test]
fn evolved_arulumaya_counts_only_face_up_earth_forms_and_creates_material() {
    let mut initial = enemy_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"arulumaya-base"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"form","card":"CP05-046"},
        {"id":"earth_up","card":"CP05-042","face_up":true},
        {"id":"earth_down","card":"CP05-042"},
        {"id":"other_up","card":"BP04-043","face_up":true}
    ]);
    let mut game = arulumaya_game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let play = first_trigger(&mut game, "CP05-046");
    resolved(answer_single_target(&mut game, play));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert!(created_in(&state, "ex", "CP05-T11"));
}

#[test]
fn lunalu_returns_a_granblue_follower_after_ordinary_limit_break() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-049"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"CP05-T14"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"returned","card":"CP05-038"},{"id":"ordinary","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let play = game
        .activate_internal_card(
            "material",
            definition("CP05-T14", AbilityKind::Activated, 0),
            PLAYER,
        )
        .unwrap();
    resolved(answer_single_target(&mut game, play));
    game.resume_internal_card().unwrap();
    let trigger = first_trigger(&mut game, "CP05-049");
    resolved(answer_single_target(&mut game, trigger));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["returned"]));
    assert_eq!(state["P1"]["cemetery"], json!(["ordinary"]));
}

#[test]
fn isaac_adds_water_material_to_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-052"}]);
    let mut game = game(&initial);
    resolved(fanfare(&mut game, "CP05-052"));
    assert!(created_in(
        &game.projection(View::Referee).unwrap(),
        "ex",
        "CP05-T10"
    ));
}

#[test]
fn ars_gloria_picks_a_matching_card_and_orders_the_other_three() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-053"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"CP05-052"},{"id":"chosen","card":"CP05-041"},
        {"id":"second","card":"CP05-038"},{"id":"third","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", PLAYER).unwrap();
    let order = awaiting(answer_single_target(&mut game, play));
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected bottom ordering");
    };
    assert_eq!(cards.len(), 3);
    resolved(answer(
        &mut game,
        &order,
        Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["chosen"]));
    assert_eq!(state["P1"]["deck"], json!(["third", "second", "first"]));
}

#[test]
fn ars_magna_from_ex_damages_the_chosen_follower_and_its_leader_then_heals() {
    let mut initial = enemy_position();
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"CP05-T08"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", PLAYER).unwrap();
    resolved(answer_single_target(&mut game, play));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
}
