#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]

use super::ability_combinations::{finish, loaded, play, position, view};
use super::*;

#[test]
fn heinlein_recovers_pp_only_for_spells_milled_from_a_short_deck() {
    let mut initial = position("BP03-045", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-125"},
        {"id":"follower","card":"BP01-173"},
        {"id":"last","card":"BP01-125"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["cemetery"], json!(["first", "follower", "last"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn evolved_rose_queen_recovers_pp_for_revealed_cards_of_original_cost_two() {
    let mut initial = ability_combinations::evolve_position("BP08-069", "BP08-070");
    initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell","card":"BP01-125"},
        {"id":"follower","card":"BP01-173"},
        {"id":"other","card":"BP01-171"}
    ]);
    let mut game = loaded(&initial);
    ability_combinations::evolve(&mut game, 2);
    let state = view(&game);
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["P1"]["hand"], json!(["spell", "follower", "other"]));
}

#[test]
fn evil_almiraj_gains_bane_when_it_mills_a_printed_ward_follower() {
    let mut initial = position("BP08-097", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"ward","card":"BP01-134"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["cemetery"], json!(["ward"]));
    assert!(state["objects"]["source"]["keywords"]
        .as_array().unwrap().contains(&json!("必殺")));
}

fn train(trainer: &str, race: bool, stat: &str, increase: i64) {
    let mut initial = position("CSD01-001", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut().unwrap().push(json!({"id":"trainer", "card":trainer}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"carrot","card":"CSD01-032"}
    ]);
    let mut game = loaded(&initial);
    if race {
        meal_race::feed(&mut game, "CSD01-001", 0, false);
        meal_race::settle(&mut game);
    }
    let before = view(&game)["objects"]["source"][stat].as_i64().unwrap();
    let step = game
        .activate_internal_card("trainer", ability(trainer, 0), PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, Some(step), &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["trainer"]["acted"], true, "the trainer paid its cost");
    assert_eq!(
        state["objects"]["source"][stat],
        before.checked_add(increase).unwrap(),
        "the trainee gains the amount for its branch"
    );
}

#[test]
fn kashimoto_increases_a_racing_followers_power_by_two() {
    train("CSD01-030", true, "power", 2);
}

#[test]
fn kiryuin_increases_an_unraced_followers_health_by_one() {
    train("CSD01-031", false, "hp", 1);
}

fn token_option(
    game: &mut Game,
    play: CardPlay,
    choose: impl Fn(&[String]) -> bool,
) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("token allocation");
    };
    let InputDomain::TokenCapacity { choices } = &question.domain else {
        panic!("token choices");
    };
    let handle = choices.iter().position(|option| choose(option)).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::TokenCapacity(u64::try_from(handle).unwrap()),
    )
    .unwrap()
}

#[test]
fn aria_splits_fairies_between_field_and_ex_and_only_the_field_fairies_gain_rush() {
    let mut initial = position("SD01-001", "hand");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"a","card":"BP01-173"}, {"id":"b","card":"BP01-173"},
        {"id":"c","card":"BP01-173"}, {"id":"d","card":"BP01-173"}
    ]);
    let mut game = loaded(&initial);
    let start = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    let CardPlay::Awaiting(ward) = start else {
        panic!("Aria Ward entry");
    };
    game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
        .unwrap();
    let fanfare = union_burst_batch1::pending(&mut game, "SD01-001");
    assert!(matches!(fanfare, CardPlay::Awaiting(_)));
    answer_aria_allocation(&mut game);
    finish(&mut game, None, &[]);
    let state = view(&game);
    let field = state["P1"]["field"].as_array().unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(field.len(), 3);
    assert_eq!(ex.len(), 5);
    let ex_fairy = &state["objects"][ex.last().unwrap().as_str().unwrap()];
    assert_eq!(ex_fairy["card"], "BP01-T03");
    assert!(ex_fairy["keywords"].as_array().is_none_or(|keywords| !keywords.contains(&json!("突進"))));
    for fairy in field.iter().skip(1) {
        let object = &state["objects"][fairy.as_str().unwrap()];
        assert_eq!(object["card"], "BP01-T03");
        assert!(object["keywords"].as_array().unwrap().contains(&json!("突進")));
    }
}

fn answer_aria_allocation(game: &mut Game) {
    use sve_engine::game::player::{
        Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer,
        observation::ObservedZone,
    };

    game.advance_player_input();
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("token allocation question");
    };
    let QuestionShape::Decision(DecisionShape::TokenCapacity(domain)) = &question.shape else {
        panic!("token allocation options");
    };
    let placements = domain.placements.as_ref().unwrap();
    assert!(placements.iter().any(Vec::is_empty), "Aria permits zero in both zones");
    let option = placements
        .iter()
        .position(|zones| zones == &[ObservedZone::Field, ObservedZone::Field, ObservedZone::Ex])
        .unwrap();
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: question.token,
                answer: Answer::TokenCapacity(u64::try_from(option).unwrap()),
            },
        ).kind,
        SubmissionKind::Applied,
        "the controller can submit the mixed allocation"
    );
}

#[test]
fn landscape_mage_pays_earth_rite_and_creates_three_golems_in_the_chosen_mix() {
    let mut game = loaded(&position("BP19-052", "hand"));
    play(&mut game, &[]);
    let CardPlay::Awaiting(payment) = game
        .activate_internal_card("source", ability("BP19-052", 1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Earth Rite payment");
    };
    let InputDomain::CostMaterials(materials) = &payment.domain else {
        panic!("Earth Rite materials");
    };
    assert_eq!(materials.members.len(), 1);
    let step = game.answer_internal_card(
        payment.actor,
        payment.token,
        Answer::CostMaterials(vec![materials.members[0].handle]),
    ).unwrap();
    let step = token_option(&mut game, step, |option| {
        option.len() == 3 && option.iter().filter(|name| *name == "防御型ゴーレム").count() == 1
    });
    finish(&mut game, Some(step), &[]);
    let state = view(&game);
    let field = state["P1"]["field"].as_array().unwrap();
    let mut golems = field.iter().skip(1)
        .map(|id| state["objects"][id.as_str().unwrap()]["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    golems.sort_unstable();
    assert_eq!(golems, ["攻撃型ゴーレム", "攻撃型ゴーレム", "防御型ゴーレム"]);
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
}

#[test]
fn lucille_chooses_artifact_destinations_and_banishes_five_distinct_costs() {
    let mut initial = position("BP10-003", "hand");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"two","card":"BP01-173"}, {"id":"three","card":"BP01-009"},
        {"id":"six","card":"BP08-069"}, {"id":"five","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"relic","card":"BP10-004"}]);
    let mut game = loaded(&initial);
    let step = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    assert!(matches!(step, CardPlay::Entered(_)));
    let fanfare = union_burst_batch1::pending(&mut game, "BP10-003");
    let next = token_option(&mut game, fanfare, |option| {
        option == ["ミスティックアーティファクト", "エンシェントアーティファクト"]
    });
    finish(&mut game, Some(next), &[]);
    let after_fanfare = view(&game);
    assert_eq!(after_fanfare["P1"]["hand"], json!(["drawn"]));
    assert_eq!(after_fanfare["P1"]["ex"].as_array().unwrap().len(), 5);
    ability_combinations::activate_card(&mut game, "BP10-003", 2, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["ex"], json!([]));
    assert!(state["P1"]["field"].as_array().unwrap().contains(&json!("relic")));
    assert_eq!(state["objects"]["source"]["acted"], true);
}
