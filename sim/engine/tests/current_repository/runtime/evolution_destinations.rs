use super::*;
use sve_engine::game::Step;

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve(game: &mut Game, definition: usize, face: u8, pp: u8, base: &str) -> Step {
    let entry = &selection_with_rules().current[base]
        .program
        .card()
        .abilities[definition];
    game.internal_rule_action(&json!({
        "do":"evolve","source":"source","evolve_card":"form",
        "definition":entry.node.id.0,"face":face,"pay":{"pp":pp}
    }))
    .unwrap()
}

#[test]
fn paula_named_list_selects_either_real_face_for_all_jp_printings() {
    for base in ["BP09-004", "BP09-P01"] {
        for (face, name) in [(0, "深緑の純心・ポーラ"), (1, "真紅の絆・ポーラ")] {
            let mut game = game(&position(base, "BP09-005"));
            let step = evolve(&mut game, 0, face, 1, base);
            assert_eq!(step.outcome, "resolved");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["name"], name);
            assert_eq!(state["objects"]["source"]["evolved_with"], "form");
            assert_eq!(state["objects"]["form"]["face"], face);
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn celia_destination_and_price_belong_to_the_selected_evolution_head() {
    for base in [
        "BP09-018",
        "BP09-SL04",
        "PR-222",
        "SP01-011",
        "SP01-SL11",
        "SP01-SP06",
        "SP01-U05",
    ] {
        for (definition, face, pp, name) in [
            (0, 0, 1, "希望の戦術家・セリア"),
            (1, 1, 4, "絶望の使者・セリア"),
        ] {
            let mut game = game(&position(base, "BP09-019"));
            assert_eq!(
                evolve(&mut game, definition, face, pp, base).outcome,
                "resolved"
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["name"], name);
            assert_eq!(state["P1"]["pp"]["current"], 10 - pp);
        }
    }
}

#[test]
fn laevateinn_uses_its_printed_name_contains_selector() {
    for base in ["BP03-056", "BP03-SL11"] {
        let mut game = game(&position(base, "BP03-057"));
        assert_eq!(evolve(&mut game, 0, 0, 1, base).outcome, "resolved");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["evolved_with"], "form");
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn wrong_names_faces_and_cross_head_prices_reject_without_reveal_or_payment() {
    for (base, form, definition, face, pp) in [
        ("BP09-004", "BP03-057", 0, 0, 1),
        ("BP09-004", "BP09-005", 0, 2, 1),
        ("BP09-018", "BP09-019", 0, 1, 1),
        ("BP09-018", "BP09-019", 1, 0, 4),
        ("BP09-018", "BP09-019", 1, 1, 1),
        ("BP03-056", "BP09-019", 0, 0, 1),
    ] {
        let mut game = game(&position(base, form));
        let before = game.projection(View::Referee).unwrap();
        let step = evolve(&mut game, definition, face, pp, base);
        assert_eq!(step.outcome, "cannot-evolve");
        assert_eq!(step.events, Vec::<Value>::new());
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}
