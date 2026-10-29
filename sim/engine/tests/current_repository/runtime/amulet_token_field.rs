#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real card scenario."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn initial(card: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw-1","card":"BP01-037"},{"id":"draw-2","card":"BP01-037"},
        {"id":"draw-3","card":"BP01-037"},{"id":"remaining","card":"BP01-037"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    value
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "amulet-token-field",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            assert!(
                domain
                    .members
                    .iter()
                    .all(|member| member.name.as_deref() == Some("ナテラの大樹")),
                "only the printed Tree name is a material"
            );
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else if let InputDomain::Targets(domain) = &question.domain {
            let target = domain
                .members
                .iter()
                .find(|member| member.player.seat == 2)
                .or_else(|| domain.members.first())
                .unwrap();
            Answer::Targets(vec![target.handle])
        } else {
            panic!("printed card target or material decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("ordinary printed card play")
    };
    if entry.resume == ResumeStage::CheckTiming {
        game.resume_internal_card().unwrap();
    }
}

fn declare(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("real printed pending ability")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    finish(game, play)
}

fn trees(game: &Game) -> Vec<String> {
    let state = game.projection(View::Referee).unwrap();
    state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|id| {
            let id = id.as_str().unwrap();
            (state["objects"][id]["card"] == "BP07-T03").then(|| id.to_owned())
        })
        .collect()
}

fn leave_tree(game: &mut Game) {
    resolved(declare(game, "BP07-T03"));
    game.resume_internal_card().unwrap();
}

#[test]
fn bp17_063_and_064_create_a_real_tree_then_evolve_and_use_its_leave_text() {
    let mut initial = initial("BP17-063");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-064"}]);
    let mut game = game(&initial);
    enter(&mut game);
    resolved(declare(&mut game, "BP17-063"));
    game.resume_internal_card().unwrap();
    let created = trees(&game);
    assert_eq!(
        created.len(),
        1,
        "the Fanfare puts a real Amulet-token directly on the field"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 7_u8,
        "the created Tree adds no play cost"
    );
    assert_eq!(
        state["P1"]["ex"],
        json!([]),
        "the Tree does not go through EX"
    );
    assert_eq!(
        state["objects"][&created[0]]["entered_this_turn"], true,
        "the Tree enters normally"
    );
    let tree_ability = selection().current["BP07-T03"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    resolved(
        game.activate_internal_card(&created[0], tree_ability, actor())
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    leave_tree(&mut game);
    let left = game.projection(View::Referee).unwrap();
    assert!(
        left["objects"].get(&created[0]).is_none(),
        "the Amulet-token ceases after leaving its permitted zones"
    );
    assert_eq!(
        left["P1"]["deck"].as_array().unwrap().len(),
        3,
        "the printed departure draws one"
    );
    assert_eq!(
        left["P1"]["hand"],
        json!([]),
        "the drawn card is discarded through its formal private choice"
    );
    assert_eq!(
        left["P1"]["cemetery"],
        json!(["draw-1"]),
        "only the discarded card remains in the cemetery"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the actual base evolves into the actual form"
    );
    resolved(declare(&mut game, "BP17-064"));
    game.resume_internal_card().unwrap();
    let evolved = game.projection(View::Referee).unwrap();
    assert_eq!(
        evolved["objects"]["source"]["power"], 4_u8,
        "the printed evolved stats apply"
    );
    assert_eq!(
        evolved["objects"]["enemy"]["damage"], 3_u8,
        "the real evolution ability deals three"
    );
    assert_eq!(
        evolved["P1"]["pp"]["current"], 5_u8,
        "base play, Tree activation and evolution pay 3+1+1"
    );
}

#[test]
fn bp17_063_field_capacity_keeps_the_source_and_only_creates_what_fits() {
    for (occupied, expected) in [(3_usize, 1_usize), (4, 0)] {
        let mut initial = initial("BP17-063");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..occupied)
                .map(|i| json!({"id":format!("ally-{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        enter(&mut game);
        resolved(declare(&mut game, "BP17-063"));
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            trees(&game).len(),
            expected,
            "capacity is checked after the source has entered"
        );
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            5,
            "the field never exceeds five"
        );
        assert_eq!(
            state["objects"]["source"]["power"], 3_u8,
            "failed token creation does not remove the played source"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 7_u8,
            "only the ordinary card play was paid"
        );
    }
}

#[test]
fn bp17_070_creates_a_tree_and_can_pay_its_two_printed_activations() {
    let mut initial = initial("BP17-070");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"tree-1","card":"BP07-T03"},{"id":"tree-2","card":"BP07-T03"},
        {"id":"other-amulet","card":"BP13-087"}
    ]);
    let mut game = game(&initial);
    enter(&mut game);
    resolved(declare(&mut game, "BP17-070"));
    game.resume_internal_card().unwrap();
    assert_eq!(trees(&game).len(), 3, "the real Fanfare adds a third Tree");
    let ability = selection().current["BP17-070"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    for expected_damage in [2_u8, 4] {
        let play = game
            .activate_internal_card("source", ability, actor())
            .unwrap();
        resolved(finish(&mut game, play));
        game.resume_internal_card().unwrap();
        leave_tree(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            expected_damage,
            "each printed activation deals two"
        );
    }
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", ability, actor())
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a third activation cannot spend the remaining Tree or PP"
    );
    assert_eq!(
        trees(&game).len(),
        1,
        "exactly two named materials were paid"
    );
    assert_eq!(
        before["P1"]["pp"]["current"], 5_u8,
        "ordinary play costs three and each activation one"
    );
    assert!(
        before["objects"].get("other-amulet").is_some(),
        "unrelated Amulets are never paid"
    );
}

#[test]
fn bp17_069_banishes_a_tree_as_its_printed_fanfare_cost_before_destroying() {
    let mut initial = initial("BP17-069");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fee","card":"BP07-T03"}]);
    let mut game = game(&initial);
    enter(&mut game);
    resolved(declare(&mut game, "BP17-069"));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 4_u8,
        "the real six-PP source was played"
    );
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["enemy"]),
        "paying the Tree destroys the selected enemy"
    );
    assert!(
        state["objects"].get("fee").is_none(),
        "the banished Amulet-token ceases"
    );
    leave_tree(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["draw-1"]),
        "paying by banish also triggers the Tree's departure draw and discard"
    );
}

#[test]
fn bp17_069_lastword_creates_the_tree_for_its_controller_on_the_opponents_turn() {
    let mut initial = initial("BP17-069");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP17-069","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-097"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real follower is destroyed by combat"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "passing the rule-owned Quick window commits combat damage"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["source"]),
        "combat destroys the actual Lastword holder"
    );
    resolved(declare(&mut game, "BP17-069"));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        trees(&game).len(),
        1,
        "the Lastword directly creates its printed Tree"
    );
    assert_eq!(
        state["P2"]["field"],
        json!([]),
        "Bane destroys the enemy attacker and the Tree belongs to the departed controller"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 10_u8,
        "Lastword creation pays no PP"
    );
}
