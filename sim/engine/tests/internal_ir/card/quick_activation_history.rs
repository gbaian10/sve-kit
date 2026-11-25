#![expect(
    clippy::panic,
    reason = "Incorrect decision shapes invalidate the Quick sequence."
)]

use super::*;

#[path = "../../support/current_fixture.rs"]
mod current_fixture;

use sve_engine::game::internal::{Answer, DecisionView, InputDomain};

fn sequence_game() -> Game {
    let (mut quick_spell, spell_author) = current_fixture::author(
        "unit-spell",
        "11111111111111111111111111111111",
        "スペル",
        vec![
            json!({"keywords":["quick"]}),
            json!({"spell":{"do":[{"life":{"who":"self","delta":1}}]}}),
        ],
    );
    quick_spell["faces"][0]["power"] = json!("-");
    quick_spell["faces"][0]["hp"] = json!("-");
    let (mut primer, primer_author) = current_fixture::author(
        "unit-primer",
        "22222222222222222222222222222222",
        "フォロワー",
        vec![json!({"activate":{
            "quick":true,"cost":{"pp":1},
            "do":[{"life":{"who":"self","delta":2}}]
        }})],
    );
    primer["faces"][0]["hp"] = json!("5");
    // CP05-002's hand activation requires an earlier Quick activation by the same player.
    let (gated, gated_author) = current_fixture::author(
        "unit-gated",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![json!({"activate":{
            "quick":true,"zone":"hand",
            "cost":{"steps":[{"pp":1},{"discard":{"ref":"this"}}]},
            "usable_if":{"history":{
                "event":"ability_played","window":"this_turn","who":"self",
                "where":{"kind":"quick_activation"},"exclude":"this_execution"
            }},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"damage":{"to":"enemy","n":4}}]
        }})],
    );
    let selection = Catalog::from_current_selection(
        &format!("{quick_spell}\n{primer}\n{gated}"),
        &json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string(),
        &json!({"documents":[],"authors":[spell_author,primer_author,gated_author]}),
    )
    .unwrap();
    let mut initial = setup(1, json!([{"id":"draw","card":"unit-primer"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-primer"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"spell","card":"unit-spell"},{"id":"gated","card":"unit-gated"},
        {"id":"second","card":"unit-gated"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"p2_draw","card":"unit-primer"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"primer","card":"unit-primer"}]);
    initial["players"]["P2"]["pp"] = json!({"current":3_u8,"max":3_u8});
    initial["players"]["P2"]["leader"]["life"] = json!(15_u8);
    Game::from_selected_effects(
        selection,
        &initial,
        &Value::Null,
        &Value::Null,
        "quick-history",
    )
    .unwrap()
}

fn choose(game: &mut Game, card: &str, gated: bool) -> CardPlay {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("expected Quick options");
    };
    assert_eq!(
        options.iter().any(|option| option.card == "unit-gated"),
        gated,
        "the gate controls the Quick menu"
    );
    let handle = options
        .iter()
        .find(|option| option.card == card)
        .unwrap()
        .handle;
    answer(game, &question, Answer::Quick(Some(handle)))
}

fn answer(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

#[test]
fn a_quick_spell_does_not_unlock_the_activation_but_a_paid_quick_activation_does() {
    let mut game = sequence_game();
    resolved(
        game.activate_internal_card("enemy", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    resolved(choose(&mut game, "unit-spell", false));
    game.resume_internal_card().unwrap();
    resolved(choose(&mut game, "unit-primer", false));
    game.resume_internal_card().unwrap();
    let CardPlay::Awaiting(targets) = choose(&mut game, "unit-gated", true) else {
        panic!("the newly eligible ability chooses its formal target");
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("expected target decision");
    };
    assert_eq!(domain.members.len(), 1);
    resolved(answer(
        &mut game,
        &targets,
        Answer::Targets(vec![domain.members[0].handle]),
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["spell", "gated"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P2"]["pp"]["current"], 0_u8);

    game.resume_internal_card().unwrap();
    let pass = game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(
        answer(&mut game, &pass, Answer::Quick(None)),
        CardPlay::Continued(_)
    ));
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let next_turn = game.projection(View::Referee).unwrap();
    assert_eq!(next_turn["turn"]["active"], "P2");
    assert!(next_turn["P2"]["pp"]["current"].as_u64().unwrap() >= 1);
    assert_eq!(next_turn["P1"]["field"], json!(["enemy"]));
    assert!(matches!(
        game.activate_internal_card("second", AbilityId(1), PlayerId { seat: 2 }),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
}
