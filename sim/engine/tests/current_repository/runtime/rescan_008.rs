use super::*;

fn evolve_at_printed_cost(base: &str, form: &str, cost: u8) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the printed PP cost permits evolution"
    );
    assert!(
        step.events.iter().any(|event| {
            event["kind"] == "進化" && event["object"] == "source" && event["with"] == "form"
        }),
        "the base follower evolves with the selected form"
    );
}

fn attack(game: &mut Game, attacker: &str, target: &str) -> String {
    let step = game
        .internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap();
    if let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    step.outcome
}

fn pass_end_phase(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    loop {
        match game.internal_card_stage() {
            Some(ResumeStage::CheckTiming) => {
                game.resume_internal_card().unwrap();
                continue;
            }
            Some(ResumeStage::Pending) => break,
            None => {}
        }
        if game.internal_pending_choice().unwrap().is_some() {
            break;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
}

fn ward_position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ward","card":card,"state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial
}

fn assert_ward_stops_leader_attack(card: &str) {
    let mut game = game(&ward_position(card));
    assert_eq!(
        attack(&mut game, "attacker", "P1.leader"),
        "cannot-attack",
        "Ward protects its controller's leader"
    );
}

#[test]
fn bp04_015_evolves_for_one_pp() {
    evolve_at_printed_cost("BP04-015", "BP04-016", 1);
}

#[test]
fn bp04_029_assail_attacks_a_follower_and_bane_destroys_it() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"assassin","card":"BP04-029"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        attack(&mut game, "assassin", "target"),
        "resolved",
        "Assail permits an immediate follower attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["target"]),
        "Bane destroys the opposing follower"
    );
}

#[test]
fn bp04_035_evolves_for_one_pp_and_ward_blocks_leader_attack() {
    evolve_at_printed_cost("BP04-035", "BP04-036", 1);
    assert_ward_stops_leader_attack("BP04-035");
}

#[test]
fn bp04_043_storm_bane_and_ward_work_in_combat() {
    let mut storm_position = draw_position();
    storm_position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"europe","card":"BP04-043","state":{"entered_this_turn":true}}]);
    let mut storm_game = game(&storm_position);
    assert_eq!(
        attack(&mut storm_game, "europe", "P2.leader"),
        "resolved",
        "Storm permits an immediate leader attack"
    );
    assert_eq!(
        storm_game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u64,
        "the evolved follower deals its printed three attack"
    );

    let mut no_storm_position = storm_position.clone();
    no_storm_position["players"]["P1"]["zones"]["field"][0]["card"] = json!("BP04-029");
    let mut no_storm_game = game(&no_storm_position);
    assert_eq!(
        attack(&mut no_storm_game, "europe", "P2.leader"),
        "cannot-attack",
        "an immediate leader attack requires Storm"
    );

    let mut bane_position = draw_position();
    bane_position["players"]["P1"]["zones"]["field"] = json!([{"id":"europe","card":"BP04-043"}]);
    bane_position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-037","state":{"acted":true}}]);
    let mut bane_game = game(&bane_position);
    assert_eq!(attack(&mut bane_game, "europe", "target"), "resolved");
    assert_eq!(
        bane_game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["target"]),
        "Bane destroys the opposing follower"
    );

    assert_ward_stops_leader_attack("BP04-043");
}

#[test]
fn bp04_068_fanfare_discards_one_card_from_its_controller() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP04-068"},
        {"id":"keep","card":"BP01-037"},
        {"id":"discard","card":"BP06-048"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare discard trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(
        options[0].line, 2,
        "the fanfare is the printed second ability"
    );
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("choose the card discarded by the fanfare");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private hand discard candidates");
    };
    let discard = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP06-048"))
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![discard.handle]),
    )
    .unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["keep"]));
    assert_eq!(state["P1"]["cemetery"], json!(["discard"]));
}

#[test]
fn bp04_068_discards_at_its_controllers_main_phase_start() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P1");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP04-068"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"keep","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"p1-draw-1","card":"BP06-048"},
        {"id":"p1-draw-2","card":"BP06-048"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"p2-draw","card":"BP01-037"}
    ]);
    let mut game = game(&initial);

    pass_end_phase(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["keep"])
    );

    pass_end_phase(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("main-start discard trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(
        options[0].line, 1,
        "the main-start ability is the printed first ability"
    );
    assert_eq!(
        pending.actor.seat, 1,
        "only the card controller's main phase triggers"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "the start-of-turn draw happens before the main-start trigger"
    );
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("choose the card discarded at main-start");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private hand discard candidates");
    };
    let discard = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP06-048"))
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![discard.handle]),
    )
    .unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["cemetery"], json!(["p1-draw-1"]));
    assert_eq!(state["P1"]["deck"], json!(["p1-draw-2"]));
}

#[test]
fn bp04_069_evolves_for_one_pp() {
    evolve_at_printed_cost("BP04-069", "BP04-070", 1);
}

#[test]
fn bp04_101_fanfare_damages_mills_two_and_draws_one() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP04-101"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"mill1","card":"BP01-037"},
        {"id":"mill2","card":"BP01-037"},
        {"id":"drawn","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].line, 2);
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u64);
    assert_eq!(state["P1"]["cemetery"], json!(["mill1", "mill2"]));
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["deck"], json!([]));
}

#[test]
fn bp04_112_evolves_for_one_pp() {
    evolve_at_printed_cost("BP04-112", "BP04-113", 1);
}

#[test]
fn bp04_119_evolves_for_two_pp_and_ward_blocks_leader_attack() {
    evolve_at_printed_cost("BP04-119", "BP04-120", 2);
    assert_ward_stops_leader_attack("BP04-119");
}

#[test]
fn bp04_127_evolves_for_one_pp() {
    evolve_at_printed_cost("BP04-127", "BP04-128", 1);
}

#[test]
fn bp04_t01_bane_destroys_a_follower_in_combat() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"viper","card":"BP04-T01"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(attack(&mut game, "viper", "target"), "resolved");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["target"])
    );
}
