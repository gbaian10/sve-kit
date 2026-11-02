#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn cards(prefix: &str, card: &str, count: usize) -> Vec<Value> {
    (0..count)
        .map(|n| json!({"id":format!("{prefix}{n}"),"card":card}))
        .collect()
}

// Collaboration titles (ウマ娘, デレマス) need the repository's rule bindings.
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "number-batch1",
    )
    .unwrap()
}

// Targets take the first legal member; cost materials take every offered card.
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(vec![domain.members[0].handle])
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(domain.members.iter().map(|m| m.handle).collect())
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    triggers(game);
}

fn triggers(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step);
}

fn evolve(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves for its printed cost"
    );
    triggers(game);
}

/// P2 has a 12-health follower and full life; P1 holds `card` in `zone` as `source`.
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    initial
}

fn evolution(base: &str, form: &str) -> Value {
    let mut initial = position(base, "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn with(initial: &mut Value, zone: &str, extra: Vec<Value>) {
    let cards = &mut initial["players"]["P1"]["zones"][zone];
    if cards.is_null() {
        *cards = json!([]);
    }
    cards.as_array_mut().unwrap().extend(extra);
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}

#[test]
fn bp07_043_doubles_the_machines_in_its_ex_area() {
    let mut initial = evolution("BP07-042", "BP07-043");
    let mut ex = cards("machine", "BP07-042", 2);
    ex.extend(cards("goblin", "BP01-171", 1));
    with(&mut initial, "ex", ex);
    let mut game = game(&initial);
    evolve(&mut game, 2);
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp08_087_fanfare_heals_twice_the_allied_amulets() {
    let mut initial = position("BP08-087", "hand");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    with(&mut initial, "field", cards("amulet", "BP01-141", 2));
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(state(&game)["P1"]["leader"]["life"], 14_u8);
}

#[test]
fn bp10_061_deals_one_hit_of_two_per_five_banished_cards() {
    let mut initial = evolution("BP10-060", "BP10-061");
    with(&mut initial, "banish", cards("gone", "BP01-171", 10));
    let mut game = game(&initial);
    evolve(&mut game, 3);
    let state = state(&game);
    // Q1312: ten banished cards make a single 4-damage hit.
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
}

#[test]
fn bp12_001_costs_five_less_per_five_natura_cards_then_destroys() {
    let mut initial = position("BP12-001", "hand");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    with(&mut initial, "cemetery", cards("natura", "BP07-008", 10));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = state(&game);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8, "Q1622: printed 11 minus 10");
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp14_089_doubles_the_allied_fanatic_followers_including_itself() {
    let mut initial = evolution("BP14-088", "BP14-089");
    with(&mut initial, "field", cards("fanatic", "BP01-146", 1));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp18_052_attack_gains_one_one_per_five_banished_cards() {
    let mut initial = evolution("BP18-051", "BP18-052");
    with(&mut initial, "banish", cards("gone", "BP01-171", 10));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the evolved follower attacks"
    );
    triggers(&mut game);
    let state = state(&game);
    assert_eq!(state["objects"]["source"]["power"], 4_u8, "printed 2 plus 2");
    assert_eq!(state["objects"]["source"]["hp"], 5_u8, "printed 3 plus 2");
}

#[test]
fn bp18_056_costs_one_less_per_five_spells_and_buffs_a_wizard() {
    let mut initial = position("BP18-056", "hand");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    with(&mut initial, "cemetery", cards("spell", "BP04-044", 10));
    with(&mut initial, "field", cards("wizard", "BP22-044", 1));
    let mut game = game(&initial);
    play(&mut game);
    let state = state(&game);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8, "Q2340: printed 3 minus 2");
    assert_eq!(state["objects"]["wizard0"]["power"], 4_u8);
}

#[test]
fn bp22_117_costs_two_less_per_allied_umamusume() {
    let mut initial = position("BP22-117", "hand");
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    with(&mut initial, "field", cards("uma", "CP01-007", 3));
    let mut game = game(&initial);
    play(&mut game);
    let state = state(&game);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8, "printed 9 minus 6");
    assert!(keyword(&state, "source", "疾走"));
}

#[test]
fn ecp02_055_evolves_beside_a_big_idol_and_ecp02_056_doubles_passion_followers() {
    let mut initial = position("ECP02-055", "hand");
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"ECP02-056"}]);
    // CP02-006 is a cost-5 Cinderella Girls follower without the Passion trait.
    with(&mut initial, "field", cards("idol", "CP02-006", 1));
    with(&mut initial, "cemetery", cards("passion", "CP02-047", 10));
    let mut game = game(&initial);
    play(&mut game);
    let state = state(&game);
    assert_eq!(state["P1"]["field"], json!(["idol0", "source"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8, "only the evolved card is Passion");
    assert!(keyword(&state, "source", "疾走"), "ten Passion cards grant Storm");
}
