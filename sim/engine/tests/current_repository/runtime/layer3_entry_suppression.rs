use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{pending, question, ub};
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position() -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP04-088"},{"id":"spare","card":"BP01-037"},
        {"id":"later","card":"CP04-085"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"suzuna","card":"CP04-017"},{"id":"misaki","card":"CP04-085"},
        {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}
    ]);
    Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "layer3-io",
    )
    .unwrap()
}

fn ub_ability() -> AbilityId {
    selection().current["CP04-088"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn assert_io_search_result(game: &Game) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 20_u8,
        "Misaki's Fanfare must not deal damage"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["spare", "later"]),
        "Suzuna's Fanfare must not discard"
    );
    let mut field = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect::<Vec<_>>();
    field.sort_unstable();
    assert_eq!(
        field,
        vec!["misaki", "source", "suzuna"],
        "Both found followers must enter beside Io"
    );
}

#[test]
fn io_searches_two_groups_without_their_fanfare_and_the_opponent_chooses_the_ub_victim() {
    let mut game = position();
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let search = question(pending(&mut game, "CP04-088"));
    let InputDomain::Search(search_domain) = &search.domain else {
        panic!("two search groups");
    };
    assert_eq!(search_domain.cards.len(), 3);
    let suzuna = search_domain
        .cards
        .iter()
        .find(|c| c.card == "CP04-017")
        .unwrap()
        .handle;
    let misaki = search_domain
        .cards
        .iter()
        .find(|c| c.card == "CP04-085")
        .unwrap()
        .handle;
    let groups = search_domain.groups.as_ref().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].handles.len(), 2);
    assert!(groups[0].handles.contains(&suzuna));
    assert!(groups[0].handles.contains(&misaki));
    assert_eq!(groups[1].handles, vec![misaki]);
    let other = search_domain
        .cards
        .iter()
        .find(|c| c.card == "BP01-037")
        .unwrap()
        .handle;
    game.answer_internal_card(
        search.actor,
        search.token,
        Answer::Search(SearchAnswer::Grouped(vec![vec![other], vec![misaki]])),
    )
    .unwrap_err();
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![suzuna], vec![misaki]])),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_io_search_result(&game);
    let ability = ub_ability();
    let victim_choice = question(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(victim_choice.actor, PlayerId { seat: 2 });
    let InputDomain::Targets(victim_domain) = &victim_choice.domain else {
        panic!("opponent choice");
    };
    assert_eq!(victim_domain.members.len(), 2);
    let picked = victim_domain.members[1].handle;
    game.answer_internal_card(
        PlayerId { seat: 1 },
        victim_choice.token,
        Answer::Targets(vec![picked]),
    )
    .unwrap_err();
    ub(&resolved(
        game.answer_internal_card(
            victim_choice.actor,
            victim_choice.token,
            Answer::Targets(vec![picked]),
        )
        .unwrap(),
    ));
    finish_checks(&mut game);
    let after_ub = game.projection(View::Referee).unwrap();
    assert_eq!(after_ub["P2"]["field"], json!(["enemy-a"]));
    assert_eq!(after_ub["P2"]["cemetery"], json!(["enemy-b"]));
    assert_eq!(after_ub["P1"]["pp"]["current"], 4_u8);
    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(matches!(
        game.play_internal_card("later", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(pending(&mut game, "CP04-085"));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u8
    );
}
