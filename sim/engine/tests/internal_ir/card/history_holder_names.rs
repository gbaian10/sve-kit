#![expect(
    clippy::panic,
    reason = "Unexpected program and choice shapes invalidate these history probes."
)]
//! 「これと同名を除く」 in current-turn movement history: saved subject names against the holder's
//! names as they are when the query is read.
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

const ALIAS: &str = "unit-spell";

fn ability(head: &Value) -> Value {
    same_name::tree(head, "unit-amulet")["cards"]["unit-amulet"]["abilities"][0].clone()
}

fn query(sel: &Value) -> Value {
    json!({"history_count":{"event":"moved","from":{"who":"self","zone":"field"},"to":"hand",
        "window":"this_turn","sel":sel}})
}

/// The reader returns one of its own followers, then gains life equal to the filtered count.
fn reader(sel: &Value, alias: bool) -> Value {
    let mut tree = same_name::tree(
        &json!({"line":1,"activate":{"cost":{"pp":0},
            "targets":[{"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
            "do":[{"move":{"ref":"t","to":"hand"}}]}}),
        "unit-amulet",
    );
    let abilities = tree["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap();
    abilities.push(ability(&json!({"line":2,"activate":{"cost":{"pp":0},
        "do":[{"life":{"who":"self","delta":query(sel)}}]}})));
    if alias {
        abilities.push(ability(
            &json!({"line":3,"static":{"effects":[{"also_named":{"name":ALIAS}}]}}),
        ));
    }
    tree
}

fn program(tree: &Value, number: &str) -> WellFormedCardProgram {
    lowerer().lower(tree).unwrap().remove(number).unwrap()
}

/// The `subject` follower carries a field-only alias.
fn board(reader_alias: bool, silenced: bool) -> (Game, AbilityId, AbilityId) {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"reader","card":"unit-amulet"},
        {"id":"subject","card":"unit-follower","state":{"silenced":silenced}}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "history-holder-names",
    )
    .unwrap();
    let reader = program(
        &reader(&json!({"not_same_name_as_this":true}), reader_alias),
        "unit-amulet",
    );
    let ids = (
        reader.card().abilities[0].node.id,
        reader.card().abilities[1].node.id,
    );
    game.load_internal_card("unit-amulet", &reader).unwrap();
    let alias = same_name::tree(
        &json!({"line":1,"static":{"zone":"field","effects":[{"also_named":{"name":ALIAS}}]}}),
        "unit-follower",
    );
    game.load_internal_card("unit-follower", &program(&alias, "unit-follower"))
        .unwrap();
    (game, ids.0, ids.1)
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn return_subject(game: &mut Game, ability: AbilityId) {
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("reader", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("return target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("return target domain");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    settle(game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["subject"]),
        "the follower returned to its owner's hand"
    );
}

fn counted(reader_alias: bool, silenced: bool) -> u64 {
    let (mut game, back, count) = board(reader_alias, silenced);
    return_subject(&mut game, back);
    resolved(
        game.activate_internal_card("reader", count, PlayerId { seat: 1 })
            .unwrap(),
    );
    settle(&mut game);
    game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_u64()
        .unwrap()
        .checked_sub(20)
        .unwrap()
}

#[test]
fn saved_subject_names_meet_the_holders_current_names_including_aliases() {
    // The field-only alias left with the follower, yet the saved names still share it.
    assert_eq!(
        counted(true, false),
        0,
        "a shared alias excludes the return"
    );
    // Silenced on the field, the follower departed under its printed name only.
    assert_eq!(counted(true, true), 1, "no alias was saved at departure");
    // Without its own alias the holder shares no name with the saved subject.
    assert_eq!(counted(false, false), 1, "the holder's aliases are read");
}

#[test]
fn history_name_exclusion_keeps_the_other_closed_filters_rejected() {
    lowerer()
        .lower(&reader(
            &json!({"not_same_name_as_this":true,"kind":"follower"}),
            false,
        ))
        .unwrap();
    for sel in [
        json!({"not_same_name_as_this":true,"name_contains":"unit"}),
        json!({"not_same_name_as_this":true,"traits_any":["a","b"]}),
        json!({"not_same_name_as_this":true,"not_class":"ウィッチ"}),
        json!({"not_same_name_as_this":true,"orig_cost_le":3}),
        json!({"not_same_name_as_this":true,"keyword":"storm"}),
        json!({"not_same_name_as_this":true,"keyword":"ward"}),
    ] {
        assert!(
            lowerer().lower(&reader(&sel, false)).is_err(),
            "{sel} stays outside the history filter"
        );
    }
}

/// The holder is a card being played: it was silenced in hand when the amulet returned, and
/// enters the resolution zone as a new generation whose alias applies again.
fn played_holder_cost(silenced_in_hand: bool, holder_alias: bool) -> u64 {
    let returner = same_name::tree(
        &json!({"line":1,"activate":{"cost":{"pp":0},
            "targets":[{"bind":"t","from":{"who":"self","zone":"field","kind":"amulet","other":true},"n":1}],
            "do":[{"move":{"ref":"t","to":"hand"}}]}}),
        "unit-amulet",
    );
    let mut holder = same_name::tree(
        &json!({"line":1,"play":{"cost_delta":{"if":{"history":{"event":"moved",
            "from":{"who":"self","zone":"field"},"to":"hand","window":"this_turn",
            "sel":{"not_same_name_as_this":true}}},"delta":-1}}}),
        "unit-follower",
    );
    if holder_alias {
        let alias = same_name::tree(
            &json!({"line":2,"static":{"effects":[{"also_named":{"name":"unit-amulet"}}]}}),
            "unit-follower",
        )["cards"]["unit-follower"]["abilities"][0]
            .clone();
        holder["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(alias);
    }
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"holder","card":"unit-follower","state":{"silenced":silenced_in_hand}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"returner","card":"unit-amulet"},
        {"id":"returned","card":"unit-amulet"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "history-holder-generation",
    )
    .unwrap();
    let returner = program(&returner, "unit-amulet");
    let back = returner.card().abilities[0].node.id;
    game.load_internal_card("unit-amulet", &returner).unwrap();
    game.load_internal_card("unit-follower", &program(&holder, "unit-follower"))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("returner", back, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("return target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("return target domain");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    settle(&mut game);
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    game.play_internal_card("holder", PlayerId { seat: 1 })
        .unwrap();
    before
        .checked_sub(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
                .as_u64()
                .unwrap(),
        )
        .unwrap()
}

#[test]
fn a_played_holder_compares_with_its_names_in_the_resolution_zone() {
    // The holder shares the returned amulet's name only through its alias.
    assert_eq!(
        played_holder_cost(false, false),
        0,
        "differently named return discounts"
    );
    assert_eq!(
        played_holder_cost(false, true),
        1,
        "the alias excludes the return"
    );
    // Silenced in hand when the return happened, the holder is read as the card being played.
    assert_eq!(
        played_holder_cost(true, true),
        1,
        "the query reads the resolution generation, not the hand one"
    );
}
