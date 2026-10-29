use super::*;

fn evolve(base: &str, form: &str, cost: u8) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["pp"]["current"] = json!(10_i32);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "printed-collaboration-evolution",
    )
    .unwrap();
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the printed evolution cost is payable"
    );
    assert!(
        step.events.iter().any(|event| event["kind"] == "進化"
            && event["object"] == "source"
            && event["with"] == "form"),
        "evolution should emit the expected source and form"
    );
}

macro_rules! evolution_test {
    ($name:ident, $base:literal, $form:literal, $cost:literal) => {
        #[test]
        fn $name() {
            evolve($base, $form, $cost);
        }
    };
}

evolution_test!(cp02_014_evolves_for_one_pp, "CP02-014", "CP02-015", 1);
evolution_test!(cp02_032_evolves_for_one_pp, "CP02-032", "CP02-033", 1);
evolution_test!(cp02_039_evolves_for_one_pp, "CP02-039", "CP02-040", 1);
evolution_test!(cp02_042_evolves_for_one_pp, "CP02-042", "CP02-043", 1);
evolution_test!(cp02_047_evolves_for_two_pp, "CP02-047", "CP02-048", 2);
evolution_test!(cp02_060_evolves_for_two_pp, "CP02-060", "CP02-061", 2);
evolution_test!(cp02_066_evolves_for_one_pp, "CP02-066", "CP02-067", 1);
evolution_test!(cp02_099_evolves_for_one_pp, "CP02-099", "CP02-100", 1);

fn play_magic_item(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(18_u8);
    initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"item","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("item", PlayerId { seat: 1 }).unwrap();
    assert!(matches!(play, CardPlay::Resolved(_)), "the spell token resolves");

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u8, "the spell heals its controller");
    assert_eq!(state["P1"]["hand"], json!(["drawn"]), "the spell draws one card");
    assert_eq!(state["P1"]["pp"]["current"], 0_u8, "the spell pays its printed four PP");
    assert!(state["objects"]["item"].is_null(), "the resolved token ceases to exist");
}

macro_rules! magic_item_test {
    ($name:ident, $card:literal) => {
        #[test]
        fn $name() {
            play_magic_item($card);
        }
    };
}

magic_item_test!(cp02_t02_magic_item_is_playable, "CP02-T02");
magic_item_test!(cp02_t03_magic_item_is_playable, "CP02-T03");
magic_item_test!(cp02_t04_magic_item_is_playable, "CP02-T04");
magic_item_test!(csd02b_t01_magic_item_is_playable, "CSD02b-T01");
magic_item_test!(cp02_t05_magic_item_is_playable, "CP02-T05");
magic_item_test!(cp02_t06_magic_item_is_playable, "CP02-T06");
magic_item_test!(cp02_t07_magic_item_is_playable, "CP02-T07");
magic_item_test!(csd02c_t01_magic_item_is_playable, "CSD02c-T01");
magic_item_test!(cp02_t08_magic_item_is_playable, "CP02-T08");
magic_item_test!(cp02_t09_magic_item_is_playable, "CP02-T09");

#[test]
fn cp02_034_quick_spell_buffs_one_ally_and_heals_its_leader() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(17_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-034"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    let first_play = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(question) = first_play else {
        panic!("spell must request a follower target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected own follower target");
    };
    assert_eq!(domain.members.len(), 1);
    let resolution = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap();
    assert!(matches!(resolution, CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 8_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn cp02_034_quick_spell_can_be_played_during_the_opponents_end_phase() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["leader"]["life"] = json!(17_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-034"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(quick.actor, PlayerId { seat: 1 });
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("expected the opposing end-phase Quick window");
    };
    assert_eq!(options.len(), 1);
    let CardPlay::Awaiting(targets) = game
        .answer_internal_card(
            quick.actor,
            quick.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap()
    else {
        panic!("the Quick spell must request its follower target");
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("expected own follower target");
    };
    assert_eq!(domain.members.len(), 1);
    assert!(matches!(
        game.answer_internal_card(
            targets.actor,
            targets.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
        CardPlay::Resolved(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 8_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}
