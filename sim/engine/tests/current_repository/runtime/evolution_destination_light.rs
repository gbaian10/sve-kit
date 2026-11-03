#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "evolution-destination-light",
    )
    .unwrap()
}

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn count(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |n| usize::try_from(n.0).unwrap())
        .min(available)
}

fn answer(domain: &InputDomain) -> Answer {
    match domain {
        InputDomain::Targets(d) => Answer::Targets(
            d.members
                .iter()
                .take(count(&d.cardinality, d.members.len()))
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::CostMaterials(d) => Answer::CostMaterials(
            d.members
                .iter()
                .take(count(&d.cardinality, d.members.len()))
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
            d.cards
                .iter()
                .take(count(&d.cardinality, d.cards.len()))
                .map(|m| m.handle)
                .collect(),
        )),
        InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::OptionalCost => Answer::OptionalCost(true),
        InputDomain::ResourceEntryPayment { .. }
        | InputDomain::Quick { .. }
        | InputDomain::BeforeDraw
        | InputDomain::Pending(_)
        | InputDomain::TriggerQuota(_)
        | InputDomain::Integer(_)
        | InputDomain::DamageReplacements(_)
        | InputDomain::DriveTrigger { .. }
        | InputDomain::SearchPartition(_)
        | InputDomain::Distribution(_)
        | InputDomain::SearchReplacement
        | InputDomain::TokenCapacity { .. }
        | InputDomain::Modes { .. } => panic!("unexpected printed choice: {domain:?}"),
    }
}

fn finish(game: &mut Game, mut play: CardPlay) {
    loop {
        while let CardPlay::Awaiting(question) = play {
            let response = answer(&question.domain);
            play = game
                .answer_internal_card(question.actor, question.token, response)
                .unwrap();
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending ability");
        };
        assert!(
            !options.is_empty(),
            "a pending question offers an occurrence"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
}

fn triggers(game: &mut Game) {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let Some(question) = game.internal_pending_choice().unwrap() else {
        return;
    };
    let InputDomain::Pending(options) = &question.domain else {
        panic!("evolution pending ability");
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

fn evolve(game: &mut Game, base: &str, definition: usize, face: u8, pp: u8) -> String {
    let entry = &selection_with_rules().current[base]
        .program
        .card()
        .abilities[definition];
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form",
        "definition":entry.node.id.0,"face":face,"pay":{"pp":pp}}),
    )
    .unwrap()
    .outcome
}

#[test]
fn celia_both_real_faces_create_their_printed_tokens_after_evolution() {
    for (definition, face, pp, names) in [
        (0, 0, 1, vec!["シールドガーディアン"]),
        (1, 1, 4, vec!["スティールナイト", "ナイト"]),
    ] {
        let mut board = game(&position("BP09-018", "BP09-019"));
        assert_eq!(
            evolve(&mut board, "BP09-018", definition, face, pp),
            "resolved"
        );
        triggers(&mut board);
        let state = board.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(
            field.len(),
            1 + names.len(),
            "only the selected face creates tokens"
        );
        for name in names {
            assert!(
                field
                    .iter()
                    .any(|id| state["objects"][id.as_str().unwrap()]["name"] == name),
                "the printed {name} enters"
            );
        }
        assert_eq!(state["P1"]["pp"]["current"], 10 - pp);
        let storm = state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"));
        assert_eq!(storm, face == 1, "only despair has storm");
    }
}

#[test]
fn orchis_second_named_destination_needs_three_cemetery_puppets() {
    for (definition, face, pp, puppets, succeeds) in
        [(0, 0, 1, 0_usize, true), (1, 1, 4, 2, false), (1, 1, 4, 3, true)]
    {
        let mut initial = position("BP08-002", "BP08-003");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..puppets)
                .map(|n| json!({"id":format!("puppet{n}"),"card":"BP08-002"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        let before = board.projection(View::Referee).unwrap();
        let outcome = evolve(&mut board, "BP08-002", definition, face, pp);
        assert_eq!(
            outcome,
            if succeeds {
                "resolved"
            } else {
                "cannot-evolve"
            }
        );
        let state = board.projection(View::Referee).unwrap();
        if succeeds {
            assert_eq!(state["objects"]["form"]["face"], face);
            assert_eq!(state["objects"]["source"]["evolved_with"], "form");
            assert_eq!(state["P1"]["pp"]["current"], 10 - pp);
        } else {
            assert_eq!(
                state, before,
                "a failed qualification spends and reveals nothing"
            );
        }
    }
}

#[test]
fn vampy_fanfare_creates_a_bat_and_second_evolution_requires_five_vampires() {
    let mut initial = position("BP09-069", "BP09-070");
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-069"}]);
    let mut board = game(&initial);
    let play = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, play);
    let state = board.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1, "fanfare creates exactly one bat");
    assert_eq!(
        state["objects"][ex[0].as_str().unwrap()]["name"],
        "フォレストバット"
    );
    for (definition, face, pp, vampires, succeeds) in
        [(0, 0, 1, 0_usize, true), (1, 1, 3, 4, false), (1, 1, 3, 5, true)]
    {
        let mut evolved = position("BP09-069", "BP09-070");
        evolved["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..vampires)
                .map(|n| json!({"id":format!("vampire{n}"),"card":"BP09-069"}))
                .collect::<Vec<_>>()
        );
        let mut attempt = game(&evolved);
        assert_eq!(
            evolve(&mut attempt, "BP09-069", definition, face, pp),
            if succeeds {
                "resolved"
            } else {
                "cannot-evolve"
            }
        );
        assert_eq!(
            attempt.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if succeeds { 10 - pp } else { 10_u8 }
        );
    }
}

#[test]
fn ceryneia_fanfare_reveals_an_amulet_and_keeps_the_other_cards_on_bottom() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-089"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"other","card":"BP01-171"},{"id":"found","card":"BP13-087"},
        {"id":"spare","card":"BP13-087"},{"id":"fourth","card":"BP01-171"},
        {"id":"fifth","card":"BP01-171"},{"id":"unseen","card":"BP13-087"}]);
    let mut board = game(&initial);
    let play = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, play);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "other", "spare", "fourth", "fifth"])
    );
    let opponent = board.projection(View::P2).unwrap();
    assert!(
        opponent["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "found"),
        "the selected amulet is revealed"
    );
}

#[test]
fn paula_first_face_takes_an_elf_spell_and_second_face_returns_other_cards() {
    for face in [0, 1] {
        let mut initial = position("BP09-004", "BP09-005");
        initial["players"]["P2"]["zones"]["field"] = json!([{ "id":"enemy", "card":"BP01-171" }]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"wrong","card":"BP01-171"},{"id":"spell","card":"BP01-015"},
            {"id":"unseen","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"friend","card":"BP01-171"}),
                json!({"id":"amulet","card":"BP13-087"}),
            ]);
        let mut board = game(&initial);
        assert_eq!(evolve(&mut board, "BP09-004", 0, face, 1), "resolved");
        triggers(&mut board);
        let state = board.projection(View::Referee).unwrap();
        if face == 0 {
            assert_eq!(state["P1"]["hand"], json!(["spell"]));
            assert_eq!(state["P1"]["deck"], json!(["wrong", "unseen"]));
        } else {
            assert_eq!(state["P1"]["field"], json!(["source"]));
            assert_eq!(state["P1"]["ex"], json!(["friend", "amulet"]));
            assert_eq!(
                state["P2"]["field"],
                json!(["enemy"]),
                "Paula cannot move an opponent card into EX"
            );
        }
    }
}

#[test]
fn ceryneia_faces_pay_an_amulet_for_healing_or_opposing_leader_damage() {
    for face in [0, 1] {
        let mut initial = position("BP09-089", "BP09-090");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"amulet","card":"BP13-087"}));
        let mut board = game(&initial);
        assert_eq!(evolve(&mut board, "BP09-089", 0, face, 1), "resolved");
        triggers(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["amulet"]),
            "the chosen amulet pays the printed cost"
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if face == 0 { 24_u8 } else { 20_u8 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if face == 1 { 16_u8 } else { 20_u8 }
        );
        let keyword = if face == 0 { "守護" } else { "必殺" };
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!(keyword)),
            "the selected face keeps {keyword}"
        );
    }
}
