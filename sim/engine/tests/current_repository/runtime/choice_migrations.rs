use super::*;

mod optional_tokens {
    #![expect(
        clippy::panic,
        reason = "Unexpected questions invalidate the printed card scenarios."
    )]
    use super::*;
    use sve_engine::game::internal::DecisionView;

    fn question(play: CardPlay) -> DecisionView {
        let CardPlay::Awaiting(question) = play else {
            panic!("expected choice")
        };
        question
    }

    fn pending_choice(game: &mut Game, number: &str) -> DecisionView {
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = pending.domain else {
            panic!("trigger")
        };
        assert_eq!(
            options.len(),
            1,
            "only the printed optional trigger is pending"
        );
        assert_eq!(
            options[0].card, number,
            "pending trigger belongs to the printed card"
        );
        question(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        )
    }

    fn finish(game: &mut Game, choice: &DecisionView, accept: bool, field: bool) {
        let play = game
            .answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Integer(Nat(u64::from(accept))),
            )
            .unwrap();
        if accept {
            let destination = question(play);
            let InputDomain::Modes { modes, .. } = &destination.domain else {
                panic!("destination alternative")
            };
            assert_eq!(
                modes,
                &vec![ModeId(1), ModeId(2)],
                "token destination offers field or EX"
            );
            resolved(
                game.answer_internal_card(
                    destination.actor,
                    destination.token,
                    Answer::Modes(vec![ModeId(if field { 1 } else { 2 })]),
                )
                .unwrap(),
            );
        } else {
            resolved(play);
        }
        assert!(
            game.internal_decision(View::Referee).is_none(),
            "the optional token creation is complete"
        );
    }

    fn optional_tree(numbers: &[&str]) {
        for number in numbers {
            for seat in ["P1", "P2"] {
                for (accept, field) in [(false, false), (true, true), (true, false)] {
                    let mut initial = draw_position();
                    initial["turn"]["active"] = json!(seat);
                    initial["players"][seat]["leader"]["life"] = json!(10_u8);
                    initial["players"][seat]["zones"]["hand"] =
                        json!([{"id":"source","card":number}]);
                    let mut game = game(&initial);
                    let mut play = game
                        .play_internal_card(
                            "source",
                            PlayerId {
                                seat: seat_number(seat),
                            },
                        )
                        .unwrap();
                    if let CardPlay::Awaiting(ward) = play {
                        assert!(
                            matches!(ward.domain, InputDomain::WardEntry { .. }),
                            "Ward entry precedes the optional Fanfare"
                        );
                        play = game
                            .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                            .unwrap();
                    }
                    assert!(
                        matches!(play, CardPlay::Entered(_)),
                        "the printed follower enters before its optional Fanfare"
                    );
                    let choice = pending_choice(&mut game, number);
                    finish(&mut game, &choice, accept, field);
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        state[seat]["field"].as_array().unwrap().len(),
                        1_usize.checked_add(usize::from(accept && field)).unwrap(),
                        "{number} only adds a field Tree when accepted for field"
                    );
                    assert_eq!(
                        state[seat]["ex"].as_array().unwrap().len(),
                        usize::from(accept && !field),
                        "{number} only adds an EX Tree when accepted for EX"
                    );
                    let zone = if field { "field" } else { "ex" };
                    if accept {
                        let id = state[seat][zone]
                            .as_array()
                            .unwrap()
                            .last()
                            .unwrap()
                            .as_str()
                            .unwrap();
                        assert_eq!(
                            state["objects"][id]["name"], "ナテラの大樹",
                            "{number} creates the printed Tree token"
                        );
                    }
                    assert_eq!(
                        state[seat]["leader"]["life"],
                        if matches!(*number, "BP07-112" | "PR-205" | "PR-338") {
                            11_u8
                        } else {
                            10
                        },
                        "Wonder Cook heals independently of the Tree choice"
                    );
                }
            }
        }
    }

    #[test]
    fn bp07_018_and_sl04_may_create_a_tree_in_either_zone() {
        optional_tree(&["BP07-018", "BP07-SL04"]);
    }
    #[test]
    fn bp07_112_and_promos_heal_even_when_tree_creation_is_declined() {
        optional_tree(&["BP07-112", "PR-205", "PR-338"]);
    }
    #[test]
    fn bp12_018_and_sl05_may_create_a_tree_in_either_zone() {
        optional_tree(&["BP12-018", "BP12-SL05"]);
    }
    #[test]
    fn bp12_027_may_create_a_tree_in_either_zone() {
        optional_tree(&["BP12-027"]);
    }
    #[test]
    fn bp17_020_and_sl06_may_create_a_tree_in_either_zone() {
        optional_tree(&["BP17-020", "BP17-SL06"]);
    }
    #[test]
    fn bp17_031_and_p07_may_create_a_tree_in_either_zone() {
        optional_tree(&["BP17-031", "BP17-P07"]);
    }

    #[test]
    fn bp22_036_and_p19_lastword_may_create_a_knight_in_either_zone() {
        for number in ["BP22-036", "BP22-P19"] {
            for (accept, field) in [(false, false), (true, true), (true, false)] {
                let mut initial = draw_position();
                initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
                let mut game = with_producer(
                    &initial,
                    Action::Destroy {
                        objects: subjects(),
                    },
                );
                activate(&mut game, "P1");
                game.resume_internal_card().unwrap();
                let choice = pending_choice(&mut game, number);
                finish(&mut game, &choice, accept, field);
                let state = game.projection(View::Referee).unwrap();
                let zone = if field { "field" } else { "ex" };
                assert_eq!(
                    state["P1"][zone].as_array().unwrap().len(),
                    usize::from(accept)
                );
                if accept {
                    let id = state["P1"][zone][0].as_str().unwrap();
                    assert_eq!(state["objects"][id]["name"], "ナイト");
                }
            }
        }
    }

    #[test]
    fn bp07_112_lastword_discards_only_natural_cards_before_drawing() {
        for number in ["BP07-112", "PR-205", "PR-338"] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"natural","card":"BP07-018"},
                {"id":"other","card":"BP01-037"}
            ]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            let mut game = with_producer(
                &initial,
                Action::Destroy {
                    objects: subjects(),
                },
            );
            activate(&mut game, "P1");
            game.resume_internal_card().unwrap();
            let mut cost = pending_choice(&mut game, number);
            if matches!(cost.domain, InputDomain::OptionalCost) {
                cost = question(
                    game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
                        .unwrap(),
                );
            }
            let InputDomain::CostMaterials(domain) = &cost.domain else {
                panic!("natural hand cost")
            };
            assert_eq!(domain.members.len(), 1);
            assert!(game.internal_decision(View::P2).is_none());
            game.answer_internal_card(cost.actor, cost.token, Answer::CostMaterials(vec![]))
                .unwrap_err();
            resolved(
                game.answer_internal_card(
                    cost.actor,
                    cost.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["other", "drawn"]));
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["source", "producer", "natural"])
            );
        }
    }
}

mod conditions {
    #![expect(
        clippy::panic,
        reason = "Unexpected decisions invalidate the migrated printed card scenarios."
    )]
    use super::*;

    #[test]
    fn bp20_116_and_p55_require_a_target_but_heal_when_sep_is_nonzero() {
        for number in ["BP20-116", "BP20-P55"] {
            for sep in [0_u8, 1] {
                for available in [false, true] {
                    let mut initial = choice_conditions::position(number, "P1");
                    initial["players"]["P1"]["sep"] = json!(sep);
                    if available {
                        initial["players"]["P2"]["zones"]["field"] =
                            json!([{"id":"enemy","card":"plain-follower"}]);
                    }
                    let mut game = choice_conditions::fixture(&initial);
                    choice_conditions::enter(&mut game, "P1");
                    let mut step = choice_conditions::trigger(&mut game);
                    if !available {
                        assert_eq!(step, CardPlay::Cancelled);
                        let state = game.projection(View::Referee).unwrap();
                        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
                        assert_eq!(state["P1"]["hand"], json!([]));
                        assert_eq!(state["P1"]["field"], json!(["source"]));
                        continue;
                    }
                    while let CardPlay::Awaiting(question) = step {
                        let answer = match &question.domain {
                            InputDomain::Targets(domain) => {
                                assert_eq!(domain.members.len(), 1);
                                Answer::Targets(vec![domain.members[0].handle])
                            }
                            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                            unexpected @ (InputDomain::MealPayment { .. }
                            | InputDomain::CostMaterials(_)
                            | InputDomain::Quick { .. }
                            | InputDomain::BeforeDraw
                            | InputDomain::Pending(_)
                            | InputDomain::TriggerQuota(_)
                            | InputDomain::Integer(_)
                            | InputDomain::Order(_)
                            | InputDomain::DamageReplacements(_)
                            | InputDomain::DriveTrigger { .. }
                            | InputDomain::Search(_)
                            | InputDomain::SearchPartition(_)
                            | InputDomain::Distribution(_)
                            | InputDomain::SearchReplacement
                            | InputDomain::TokenCapacity { .. }
                            | InputDomain::Modes { .. }
                            | InputDomain::OptionalCost) => panic!(
                                "unexpected decision instead of mandatory target or Ward entry: {unexpected:?}"
                            ),
                        };
                        step = game
                            .answer_internal_card(question.actor, question.token, answer)
                            .unwrap();
                    }
                    resolved(step);
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        state["objects"]["enemy"]["damage"],
                        if sep == 0 { 5_u8 } else { 0 }
                    );
                    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
                }
            }
        }
    }

    fn finish_observed_look(game: &mut Game, mut step: CardPlay, take: bool) {
        while let CardPlay::Awaiting(question) = step {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => {
                    assert_eq!(
                        domain.members.len(),
                        1,
                        "only the observed Hachigoku card is eligible"
                    );
                    assert!(
                        game.internal_decision(View::P2).is_none(),
                        "the observed choice is private to its owner"
                    );
                    Answer::Targets(if take {
                        vec![domain.members[0].handle]
                    } else {
                        vec![]
                    })
                }
                InputDomain::Order(cards) => {
                    let observer = game.projection(View::P2).unwrap();
                    if take {
                        assert_eq!(
                            observer["objects"]["match"]["name"], "hachigoku",
                            "the chosen card is public while the remainder is ordered"
                        );
                    }
                    for id in ["other-one", "other-two", "other-three"] {
                        assert!(
                            observer["objects"][id].is_null(),
                            "unselected Look cards remain private"
                        );
                    }
                    Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
                }
                unexpected @ (InputDomain::MealPayment { .. }
                | InputDomain::CostMaterials(_)
                | InputDomain::WardEntry { .. }
                | InputDomain::Quick { .. }
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
                | InputDomain::DamageReplacements(_)
                | InputDomain::DriveTrigger { .. }
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::TokenCapacity { .. }
                | InputDomain::Modes { .. }
                | InputDomain::OptionalCost) => panic!(
                    "unexpected decision instead of observed choice or bottom order: {unexpected:?}"
                ),
            };
            step = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(step);
    }

    #[test]
    fn bp19_116_and_promo_reveal_only_an_observed_hachigoku_and_draw_from_turn_eight() {
        for number in ["BP19-116", "PR-527"] {
            for take in [false, true] {
                for turn in [7_u8, 8] {
                    let mut initial = choice_conditions::position(number, "P1");
                    initial["turn"]["elapsed_turns"]["P1"] = json!(turn);
                    initial["players"]["P1"]["zones"]["deck"] = json!([
                        {"id":"match","card":"hachigoku"},{"id":"other-one","card":"plain-spell"},
                        {"id":"other-two","card":"plain-spell"},{"id":"other-three","card":"plain-spell"},
                        {"id":"tail","card":"hachigoku"}
                    ]);
                    let mut game = choice_conditions::fixture(&initial);
                    choice_conditions::enter(&mut game, "P1");
                    let step = choice_conditions::trigger(&mut game);
                    finish_observed_look(&mut game, step, take);
                    let observer = game.projection(View::P2).unwrap();
                    assert!(
                        observer["objects"]["match"].is_null(),
                        "resolution does not leave a trackable private hand slot"
                    );
                    assert_eq!(
                        observer["known_cards"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|card| card["id"] == "match"),
                        take,
                        "only the actually revealed card remains historical knowledge"
                    );
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        state["P1"]["hand"],
                        if take { json!(["match"]) } else { json!([]) }
                    );
                    assert_eq!(
                        state["P1"]["deck"],
                        if take {
                            json!(["tail", "other-three", "other-two", "other-one"])
                        } else {
                            json!(["tail", "other-three", "other-two", "other-one", "match"])
                        }
                    );
                    game.resume_internal_card().unwrap();
                    resolved(
                        game.activate_internal_card(
                            "source",
                            selection().current[number]
                                .program
                                .card()
                                .abilities
                                .iter()
                                .find(|entry| entry.source.line == 2)
                                .unwrap()
                                .node
                                .id,
                            PlayerId { seat: 1 },
                        )
                        .unwrap(),
                    );
                    let after_activation = game.projection(View::Referee).unwrap();
                    assert_eq!(after_activation["P1"]["cemetery"], json!(["source"]));
                    assert_eq!(
                        after_activation["P1"]["hand"].as_array().unwrap().len(),
                        usize::from(take) + usize::from(turn == 8)
                    );
                    assert_eq!(
                        after_activation["P1"]["deck"].as_array().unwrap().first(),
                        Some(&json!(if turn == 8 { "other-three" } else { "tail" }))
                    );
                }
            }
        }
    }

    #[test]
    fn bp22_111_and_sl27_destroy_then_create_shadow_general_only_without_sep() {
        for number in ["BP22-111", "BP22-SL27"] {
            for sep in [0_u8, 1] {
                let mut initial = choice_conditions::position(number, "P1");
                initial["players"]["P1"]["sep"] = json!(sep);
                initial["players"]["P2"]["zones"]["field"] =
                    json!([{"id":"enemy","card":"banquet-amulet"}]);
                let mut game = choice_conditions::fixture(&initial);
                let mut step = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                while let CardPlay::Awaiting(question) = step {
                    let answer = match &question.domain {
                        InputDomain::Targets(domain) => {
                            assert_eq!(domain.members.len(), 1);
                            Answer::Targets(vec![domain.members[0].handle])
                        }
                        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                        unexpected @ (InputDomain::MealPayment { .. }
                        | InputDomain::CostMaterials(_)
                        | InputDomain::Quick { .. }
                        | InputDomain::BeforeDraw
                        | InputDomain::Pending(_)
                        | InputDomain::TriggerQuota(_)
                        | InputDomain::Integer(_)
                        | InputDomain::Order(_)
                        | InputDomain::DamageReplacements(_)
                        | InputDomain::DriveTrigger { .. }
                        | InputDomain::Search(_)
                        | InputDomain::SearchPartition(_)
                        | InputDomain::Distribution(_)
                        | InputDomain::SearchReplacement
                        | InputDomain::TokenCapacity { .. }
                        | InputDomain::Modes { .. }
                        | InputDomain::OptionalCost) => panic!(
                            "unexpected decision instead of destruction target or token Ward: {unexpected:?}"
                        ),
                    };
                    step = game
                        .answer_internal_card(question.actor, question.token, answer)
                        .unwrap();
                }
                resolved(step);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
                let field = state["P1"]["field"].as_array().unwrap();
                assert_eq!(field.len(), usize::from(sep == 0));
                if sep == 0 {
                    let id = field[0].as_str().unwrap();
                    assert_eq!(state["objects"][id]["name"], "シャドウジェネラル");
                    for name in ["必殺", "守護"] {
                        assert!(
                            state["objects"][id]["keywords"]
                                .as_array()
                                .unwrap()
                                .contains(&json!(name))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cp01_079_and_all_printings_require_a_target_and_check_other_followers() {
        for number in ["CP01-079", "CP01-SL19", "CP01-SP19"] {
            for follower in [false, true] {
                for available in [false, true] {
                    let mut initial = choice_conditions::position(number, "P1");
                    initial["players"]["P1"]["zones"]["hand"] = json!([]);
                    initial["players"]["P1"]["zones"]["field"] = json!([
                        {"id":"source","card":number},{"id":"other","card":if follower{"plain-follower"}else{"banquet-amulet"}}
                    ]);
                    if available {
                        initial["players"]["P2"]["zones"]["field"] =
                            json!([{"id":"enemy","card":"plain-follower"}]);
                    }
                    let mut game = choice_conditions::fixture(&initial);
                    game.internal_rule_action(
                        &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
                    )
                    .unwrap();
                    let step = choice_conditions::trigger(&mut game);
                    if available {
                        let question = choice_conditions::awaiting(step);
                        let InputDomain::Targets(domain) = &question.domain else {
                            panic!("required attack target")
                        };
                        assert_eq!(domain.members.len(), 1);
                        resolved(
                            game.answer_internal_card(
                                question.actor,
                                question.token,
                                Answer::Targets(vec![domain.members[0].handle]),
                            )
                            .unwrap(),
                        );
                    } else {
                        assert_eq!(
                            step,
                            CardPlay::Cancelled,
                            "Q398 forbids the ability without a target"
                        );
                    }
                    let state = game.projection(View::Referee).unwrap();
                    let destroys = !follower && available;
                    assert_eq!(
                        state["P2"]["leader"]["life"],
                        if destroys { 17_u8 } else { 20 }
                    );
                    assert_eq!(
                        state["P2"]["cemetery"],
                        if destroys {
                            json!(["enemy"])
                        } else {
                            json!([])
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn bp16_040_and_sl12_exclude_itself_and_banquet_amulets_from_draw_and_end_damage() {
        for number in ["BP16-040", "BP16-SL12"] {
            let mut initial = choice_conditions::position(number, "P1");
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"ally-one","card":"banquet"},{"id":"ally-two","card":"banquet"},
                {"id":"amulet","card":"banquet-amulet"}
            ]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"banquet"}]);
            let mut game = choice_conditions::fixture(&initial);
            choice_conditions::enter(&mut game, "P1");
            resolved(choice_conditions::trigger(&mut game));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["first", "second"])
            );
            game.resume_internal_card().unwrap();
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            resolved(choice_conditions::trigger(&mut game));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["leader"]["life"], 18_u8);
            assert_eq!(state["P1"]["leader"]["life"], 10_u8);
        }
    }

    #[test]
    fn bp10_069_discards_before_four_damage_and_draws_only_for_arcana() {
        for card in ["arcana", "plain-follower"] {
            let mut initial = choice_conditions::position("BP10-068", "P1");
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payment","card":card}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-068"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP10-069"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"plain-follower"}]);
            let mut game = choice_conditions::fixture(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let mut step = choice_conditions::trigger(&mut game);
            while let CardPlay::Awaiting(question) = step {
                let answer = match &question.domain {
                    InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                    InputDomain::CostMaterials(domain) => {
                        Answer::CostMaterials(vec![domain.members[0].handle])
                    }
                    InputDomain::OptionalCost => Answer::OptionalCost(true),
                    unexpected @ (InputDomain::MealPayment { .. }
                    | InputDomain::WardEntry { .. }
                    | InputDomain::Quick { .. }
                    | InputDomain::BeforeDraw
                    | InputDomain::Pending(_)
                    | InputDomain::TriggerQuota(_)
                    | InputDomain::Integer(_)
                    | InputDomain::Order(_)
                    | InputDomain::DamageReplacements(_)
                    | InputDomain::DriveTrigger { .. }
                    | InputDomain::Search(_)
                    | InputDomain::SearchPartition(_)
                    | InputDomain::Distribution(_)
                    | InputDomain::SearchReplacement
                    | InputDomain::TokenCapacity { .. }
                    | InputDomain::Modes { .. }) => panic!(
                        "unexpected decision instead of target or discard cost: {unexpected:?}"
                    ),
                };
                step = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
            resolved(step);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
            assert_eq!(state["P1"]["cemetery"], json!(["payment"]));
            assert_eq!(
                state["P1"]["hand"],
                if card == "arcana" {
                    json!(["first"])
                } else {
                    json!([])
                }
            );
        }
    }
    #[test]
    fn bp10_069_cannot_discard_without_an_opposing_follower() {
        let mut initial = choice_conditions::position("BP10-068", "P1");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payment","card":"arcana"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-068"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-069"}]);
        let mut game = choice_conditions::fixture(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        let intent = choice_conditions::awaiting(choice_conditions::trigger(&mut game));
        assert!(matches!(intent.domain, InputDomain::OptionalCost));
        assert_eq!(
            game.answer_internal_card(intent.actor, intent.token, Answer::OptionalCost(true))
                .unwrap(),
            CardPlay::Cancelled,
            "Q1320 rejects the declared ability before selecting or discarding any payment"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["payment"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
    }
}

mod medusiana {
    use super::super::selection_shapes::{awaiting, enter, position, trigger};
    use super::*;

    fn with_killer(initial: &Value) -> Game {
        static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
        let selected = SELECTED.get_or_init(|| {
            let (mut snapshot, keywords, mut input) = inputs().clone();
            for (number, traits) in [("devil", vec!["魔界"]), ("two", vec![]), ("producer", vec![])] {
                snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"0","power":"2","hp":"30","text":null,"sections":[]}]}).to_string());
                snapshot.push('\n');
                input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
            }
            repository::selection_with_rules_from(&snapshot, &keywords, &input)
        });
        let mut objects = subjects();
        if let Selector::Zone { players, .. } = &mut objects {
            players.relation = PlayerRelation::SelfPlayer;
            players.cardinality = Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            };
        }
        let mut game = with_selected_producer(initial, Action::Destroy { objects }, selected);
        let empty = WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap();
        for number in ["devil", "two"] {
            game.load_internal_card(number, &empty).unwrap();
        }
        game
    }

    #[test]
    fn bp12_p28_and_t05_heal_for_own_devil_followers_then_opponent_chooses_on_departure() {
        for number in ["BP12-P28", "BP12-T05"] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["field"] = json!([{ "id":"ally","card":"devil" }]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"enemy-one","card":"devil"},{"id":"enemy-two","card":"two"}
            ]);
            let mut game = with_killer(&initial);
            enter(&mut game);
            resolved(trigger(&mut game, number));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                12_u8
            );
            game.resume_internal_card().unwrap();
            activate(&mut game, "P1");
            game.resume_internal_card().unwrap();
            let choice = awaiting(trigger(&mut game, number));
            assert_eq!(choice.actor, PlayerId { seat: 2 });
            let InputDomain::Targets(domain) = &choice.domain else {
                panic!("opponent sacrifice")
            };
            assert_eq!(domain.members.len(), 2);
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Targets(vec![domain.members[1].handle]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["field"], json!(["enemy-one"]));
            assert_eq!(state["P2"]["cemetery"], json!(["enemy-two"]));
        }
    }
}

mod original_cost {
    use super::super::selection_shapes::passion_sum;

    #[test]
    fn ecp02_sp02_and_u02_sum_original_passion_cost_at_ten_and_twenty() {
        passion_sum(&["ECP02-SP02", "ECP02-U02"]);
    }
}
