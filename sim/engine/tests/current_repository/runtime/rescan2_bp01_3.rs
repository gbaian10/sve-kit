#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().map(|card| card.handle).collect(),
            ))
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("formal target or Search");
            };
            Answer::Targets(vec![domain.members[0].handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed Fanfare or evolution trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed trigger is pending"
    );
    let fired = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, fired);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed base evolves for two PP"
    );
    trigger(game);
}

#[test]
fn bp01_040_fanfare_searches_only_a_ninja_and_can_find_nothing() {
    for found in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-040"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"other","card":"BP01-171"}]);
        if found {
            initial["players"]["P1"]["zones"]["deck"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"found","card":"BP01-040"}));
        }
        let mut game = game(&initial);
        let entered = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entered);
        trigger(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if found { json!(["found"]) } else { json!([]) }
        );
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 1);
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    }
}

#[test]
fn bp01_136_normal_evolution_searches_only_an_amulet_to_hand() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-135"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-136"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"other","card":"BP01-171"},{"id":"found","card":"BP19-108"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["other"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

fn crest_case(which: usize) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-041"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let abilities = selection().current["BP01-041"]
        .program
        .card()
        .abilities
        .iter()
        .filter(|entry| entry.node.kind == AbilityKind::Activated)
        .map(|entry| entry.node.id)
        .collect::<Vec<_>>();
    let activated = game
        .activate_internal_card("source", abilities[which], PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, activated);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["source"]["acted"], true,
        "each printed ability rests its source as a cost"
    );
    if which == 0 {
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "creating the Knight costs one PP"
        );
        let knight = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|id| {
                let id = id.as_str().unwrap();
                (id != "source").then_some(id.to_owned())
            })
            .unwrap();
        assert_eq!(
            state["objects"][&knight]["name"], "ナイト",
            "the named token is created"
        );
        assert_eq!(
            state["objects"][&knight]["power"], 1_u8,
            "the Knight keeps its printed power"
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":knight,"target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the created Knight gains Storm and attacks immediately"
        );
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            19_u8,
            "the Knight deals one attack damage"
        );
    } else if which == 1 {
        assert_eq!(
            state["P1"]["pp"]["current"], 8_u8,
            "resting the target costs two PP"
        );
        assert_eq!(
            state["objects"]["enemy"]["acted"], true,
            "the selected enemy is rested"
        );
    } else {
        assert_eq!(state["P1"]["pp"]["current"], 7_u8, "drawing costs three PP");
        assert_eq!(
            state["P1"]["hand"],
            json!(["drawn"]),
            "the final ability draws one card"
        );
    }
}

#[test]
fn bp01_041_each_printed_activation_pays_its_own_cost_and_resolves() {
    for which in 0..3 {
        crest_case(which);
    }
}

#[test]
fn bp01_027_fills_available_field_slots_and_puts_the_remaining_guards_in_ex() {
    for (others, field_tokens, ex_tokens) in [
        (0_usize, 3_usize, 0_usize),
        (1, 3, 0),
        (2, 2, 1),
        (3, 1, 2),
        (4, 0, 3),
    ] {
        let mut initial = draw_position();
        let mut field = vec![json!({"id":"source","card":"BP01-026"})];
        for n in 0..others {
            field.push(json!({"id":format!("existing-{n}"),"card":if n == 0 { "BP19-108" } else { "BP01-171" }}));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-027"}]);
        let mut game = game(&initial);
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            1 + others + field_tokens
        );
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), ex_tokens);
        let placed = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == "乙姫お守り隊")
            .count();
        assert_eq!(placed, field_tokens);
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .iter()
                .all(|id| state["objects"][id.as_str().unwrap()]["name"] == "乙姫お守り隊")
        );
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["ex"], json!([]));
    }
}
