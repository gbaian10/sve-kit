#![expect(
    clippy::panic,
    reason = "Unrelated questions indicate that the printed scenario took a different path."
)]

use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

fn release_position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"both_stages","card":"CP05-110"}, {"id":"regular_stage","card":"CP05-P56"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"gold","card":"CP05-T15"}, {"id":"dark","card":"CP05-T14"}
    ]);
    initial
}

fn activate(game: &mut Game, object: &str, printing: &str) -> sve_engine::Result<CardPlay> {
    let ability = selection_with_rules().current[printing]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    game.activate_internal_card(object, ability, ACTOR)
}

fn choose_card(game: &mut Game, play: CardPlay, printing: &str) {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a card selection");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected target candidates");
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(printing))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn pending_ability(game: &mut Game, printing: &str, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a printed pending ability");
    };
    let option = options
        .iter()
        .find(|option| option.card == printing && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn play_source(game: &mut Game, printing: &str, fanfare_line: u64) {
    assert!(
        matches!(
            game.play_internal_card("source", ACTOR).unwrap(),
            CardPlay::Entered(_)
        ),
        "the card must enter before its printed Fanfare resolves"
    );
    resolved(pending_ability(game, printing, fanfare_line));
    game.resume_internal_card().unwrap();
}

fn release(game: &mut Game, material: &str, printing: &str, bearer: &str) {
    let activation = activate(game, material, printing).unwrap();
    choose_card(game, activation, bearer);
}

fn ex_printings(projection: &Value) -> Vec<&str> {
    projection["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| {
            projection["objects"][object.as_str().unwrap()]["card"]
                .as_str()
                .unwrap()
        })
        .collect()
}

#[test]
fn cp05_061_requires_three_successful_release_stages_before_activation() {
    let mut initial = release_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P45"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wind","card":"CP05-064"}, {"id":"tail","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    play_source(&mut game, "CP05-P45", 1);
    assert_eq!(
        ex_printings(&game.projection(View::Referee).unwrap()),
        ["CP05-T15", "CP05-T14", "CP05-T12", "CP05-T12"],
        "the printed Fanfare puts both Wind elements into EX in one activation"
    );

    release(&mut game, "gold", "CP05-T15", "CP05-110");
    let before = game.projection(View::Referee).unwrap();
    activate(&mut game, "source", "CP05-P45").unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "two completed stages leave the source on the field and the top card hidden"
    );

    release(&mut game, "dark", "CP05-T14", "CP05-P56");
    let activation = activate(&mut game, "source", "CP05-P45").unwrap();
    assert!(
        game.internal_decision(View::P2).is_none(),
        "Look candidates are private"
    );
    choose_card(&mut game, activation, "CP05-064");
    let result = game.projection(View::Referee).unwrap();
    assert_eq!(result["P1"]["cemetery"], json!(["source"]));
    assert_eq!(result["P1"]["hand"], json!(["wind"]));
    assert_eq!(result["P1"]["deck"], json!(["tail"]));
    let opponent = game.projection(View::P2).unwrap();
    assert!(
        opponent["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "wind"),
        "the selected Wind card becomes public when added"
    );
}

#[test]
fn cp05_064_reduces_its_printed_cost_by_regular_and_final_release_stages() {
    let mut initial = release_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P48"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    release(&mut game, "gold", "CP05-T15", "CP05-110");
    release(&mut game, "dark", "CP05-T14", "CP05-P56");
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    play_source(&mut game, "CP05-P48", 2);
    let result = game.projection(View::Referee).unwrap();
    assert_eq!(
        result["P1"]["pp"]["current"],
        before.checked_sub(1).unwrap(),
        "the four-PP follower costs one after three successful stages"
    );
    assert_eq!(result["P1"]["hand"], json!(["drawn"]));
    assert_eq!(ex_printings(&result), ["CP05-T12"]);
}
