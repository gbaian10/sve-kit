use super::*;

fn definition(number: &str, zone: ZoneKind) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated && ability.node.active_zones == [zone]
        })
        .unwrap()
        .node
        .id
}

// The inert namesake isolates this card's payment from the unadmitted material's abilities.
fn named_material_game(initial: &Value) -> Game {
    static MATERIAL: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = MATERIAL.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        snapshot.push_str(&json!({"number":"zone-material","faces":[{
            "name":"カオスルーラー・アイシィレンドリング","card_class":"ニュートラル",
            "card_type":"フォロワー","cost":"0","power":"1","hp":"1","traits":[],"text":null,"sections":[]
        }]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "synthetic-zone-material.yaml",
            "format: 1\nkind: effect_set\ncards:\n  zone-material:\n    status: complete\n    abilities: []\n"
        ]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    });
    let mut board = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "named-material",
    )
    .unwrap();
    board
        .load_internal_card(
            "zone-material",
            &WellFormedCardProgram::check(CardProgram {
                abilities: vec![],
                card_play_costs: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    board
}

#[test]
fn bp21_070_discards_the_activated_copy_and_draws_after_payment() {
    for (seat, number) in [("P1", "BP21-070"), ("P2", "PR-589")] {
        let actor = PlayerId {
            seat: seat_number(seat),
        };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([
            {"id":"source","card":number}, {"id":"copy","card":number}
        ]);
        initial["players"][seat]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut board = game(&initial);
        let result = resolved(
            board
                .activate_internal_card("source", definition(number, ZoneKind::Hand), actor)
                .unwrap(),
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["cemetery"], json!(["source"]));
        assert_eq!(state[seat]["hand"], json!(["copy", "drawn"]));
        assert_eq!(state[seat]["pp"]["current"], 9_u8);
        for zone in ["field", "ex", "cemetery"] {
            let mut wrong = initial.clone();
            wrong["players"][seat]["zones"][zone] = wrong["players"][seat]["zones"]["hand"].take();
            wrong["players"][seat]["zones"]["hand"] = json!([]);
            let mut invalid_board = game(&wrong);
            let before = format!("{invalid_board:?}");
            invalid_board
                .activate_internal_card("source", definition(number, ZoneKind::Hand), actor)
                .unwrap_err();
            assert!(
                format!("{invalid_board:?}") == before,
                "wrong-zone activation is atomic"
            );
        }
        initial["players"][seat]["pp"]["current"] = json!(0_u8);
        let mut unpaid = game(&initial);
        let before = format!("{unpaid:?}");
        unpaid
            .activate_internal_card("source", definition(number, ZoneKind::Hand), actor)
            .unwrap_err();
        assert!(format!("{unpaid:?}") == before, "insufficient PP is atomic");
    }
}

#[test]
fn bp10_101_hand_quick_heals_and_leaves_no_reusable_window_candidate() {
    let mut initial = draw_position();
    initial["players"]["P2"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"private-hand-priest","card":"BP10-101"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    let opponent = board.projection(View::P1).unwrap();
    assert!(opponent["awaiting"]["domain"].is_null());
    assert!(!opponent.to_string().contains("private-hand-priest"));
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].zone, "hand");
    assert_eq!(options[0].card, "BP10-101");
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 12_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["private-hand-priest"]));
    assert_eq!(state["P2"]["pp"]["current"], 9_u8);
    board.resume_internal_card().unwrap();
    let next = board.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(next.domain, InputDomain::Quick { options: remaining, .. } if remaining.is_empty())
    );
}

#[test]
fn bp11_075_banishes_its_cemetery_source_and_named_material_to_act_a_follower() {
    for number in ["BP11-075", "BP11-P17", "EBD03-011", "PR-541"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"source","card":number}, {"id":"material","card":"zone-material"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut board = named_material_game(&initial);
        let mut play = board
            .activate_internal_card(
                "source",
                definition(number, ZoneKind::Cemetery),
                PlayerId { seat: 1 },
            )
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let answer = if matches!(question.domain, InputDomain::CostMaterials(_)) {
                Answer::CostMaterials(vec![0])
            } else {
                assert!(matches!(question.domain, InputDomain::Targets(_)));
                Answer::Targets(vec![0])
            };
            play = board
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(play);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["banish"], json!(["source", "material"]));
        assert_eq!(state["objects"]["enemy"]["acted"], true);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":number}]);
        let mut insufficient = named_material_game(&initial);
        let unchanged = insufficient.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = insufficient
            .activate_internal_card(
                "source",
                definition(number, ZoneKind::Cemetery),
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("target before material payment")
        };
        assert!(matches!(question.domain, InputDomain::Targets(_)));
        insufficient
            .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap_err();
        assert!(insufficient.internal_decision(View::Referee).is_none());
        let after = insufficient.projection(View::Referee).unwrap();
        assert_eq!(after["P1"], unchanged["P1"]);
        assert_eq!(after["P2"], unchanged["P2"]);
        assert_eq!(after["objects"], unchanged["objects"]);
        let before_stale = format!("{insufficient:?}");
        insufficient
            .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap_err();
        assert!(
            format!("{insufficient:?}") == before_stale,
            "a rolled-back token stays invalid"
        );
        let CardPlay::Awaiting(retry) = insufficient
            .activate_internal_card(
                "source",
                definition(number, ZoneKind::Cemetery),
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("a fresh activation can ask for targets")
        };
        assert!(retry.token.epoch > question.token.epoch);
    }
}
