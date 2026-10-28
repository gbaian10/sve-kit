#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real-card scenario."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the real card's trigger");
    };
    assert_eq!(options.len(), 1_usize, "one real-card trigger is pending");
    assert_eq!(
        options[0].card, card,
        "the real card supplies its printed ability"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
}

#[test]
fn bp17_082_real_evolution_heals_for_each_printing() {
    for form in ["BP17-082", "BP17-P20", "PR-487"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-081"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
        let mut insufficient = initial.clone();
        insufficient["players"]["P1"]["pp"]["current"] = json!(1_u8);
        let mut denied = game(&insufficient);
        if let Ok(step) = denied.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        ) {
            assert_ne!(step.outcome, "resolved");
        }
        assert_eq!(
            denied.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20_u8
        );
        assert_eq!(
            denied.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            1_u8
        );
        let mut game = game(&initial);
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        assert!(
            step.events
                .iter()
                .any(|event| event["kind"] == "進化" && event["with"] == "form")
        );
        trigger(&mut game, form);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 22_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["P1"]["evolve_deck"], json!([]));
    }
}

#[test]
fn bp17_t10_creation_heals_then_rush_attacks_only_a_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let action = Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId("エスカマリの巡回者".into()),
            name: Expr::Literal(CardName("エスカマリの巡回者".into())),
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    };
    let mut game = with_producer(&initial, action);
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game, "BP17-T10");
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    let tokens = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP17-T10")
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 1_usize);
    let source = tokens[0].as_str().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":source,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":source,"target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "a valid Quick Pass resumes the stored attack"
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(after["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp17_087_fanfare_damages_and_draws_then_lastword_creates_bat_in_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP17-087"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"}
    ]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    assert!(matches!(
        game.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    trigger(&mut game, "BP17-087");
    game.resume_internal_card().unwrap();
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["leader"]["life"], 19_u8);
    assert_eq!(entered["P2"]["leader"]["life"], 20_u8);
    assert_eq!(entered["P1"]["hand"], json!(["first"]));
    assert_eq!(entered["P1"]["deck"], json!(["second"]));
    assert_eq!(entered["P1"]["pp"]["current"], 0_u8);
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game, "BP17-087");
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1_usize);
    let bat = &state["objects"][ex[0].as_str().unwrap()];
    assert_eq!(bat["card"], "BP01-T15");
    assert_eq!(bat["power"], 1_u8);
    assert_eq!(bat["hp"], 1_u8);
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["ex"], json!([]));
}

#[test]
fn bp17_014_real_evolution_heals_and_keeps_ward() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-013"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-014"}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}}),
        )
        .unwrap();
    assert_eq!(step.outcome, "resolved");
    assert!(
        step.events
            .iter()
            .any(|event| event["kind"] == "進化" && event["with"] == "form")
    );
    trigger(&mut game, "BP17-014");
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["leader"]["life"], 24_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
    assert_eq!(state["objects"]["source"]["hp"], 8_u8);
    assert_eq!(state["objects"]["source"]["keywords"], json!(["守護"]));
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
}

fn cast_repair(game: &mut Game) {
    let before = game.projection(View::Referee).unwrap();
    let ex = before["P1"]["ex"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        1_usize,
        "the mother creates exactly one spell token in EX"
    );
    let id = ex[0].as_str().unwrap();
    assert_eq!(
        before["objects"][id]["card"], "BP07-T02",
        "the selected complete Repair Mode template is used"
    );
    assert_eq!(
        before["P1"]["pp"]["current"], 1_u8,
        "one PP remains for the generated spell's printed cost"
    );
    resolved(game.play_internal_card(id, actor()).unwrap());
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["leader"]["life"], 21_u8,
        "the real Repair Mode heals its controller"
    );
    assert_eq!(
        after["P2"]["leader"]["life"], 20_u8,
        "Repair Mode does not heal an opponent"
    );
    assert_eq!(
        after["P1"]["pp"]["current"], 0_u8,
        "the spell pays its intrinsic one PP"
    );
    assert_eq!(
        after["P1"]["ex"],
        json!([]),
        "the spell leaves EX after being played"
    );
    assert!(
        after["objects"][id].is_null(),
        "the resolved spell token ceases to exist"
    );
}

fn fanfare_repair(card: &str, budget: u8) {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(budget);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    let mut game = game(&initial);
    let mut play = game.play_internal_card("source", actor()).unwrap();
    if let CardPlay::Awaiting(question) = play {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "only printed Ward needs an entry posture decision"
        );
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the complete real follower enters after paying its printed cost"
    );
    trigger(&mut game, card);
    game.resume_internal_card().unwrap();
    cast_repair(&mut game);
}

#[test]
fn bp17_041_fanfare_creates_a_playable_repair_mode() {
    fanfare_repair("BP17-041", 3_u8);
}

#[test]
fn bp17_052_ward_fanfare_creates_a_playable_repair_mode() {
    fanfare_repair("BP17-052", 2_u8);
}

#[test]
fn bp17_100_fanfare_creates_a_playable_repair_mode() {
    fanfare_repair("BP17-100", 4_u8);
}

#[test]
fn bp17_049_lastword_creates_a_playable_repair_mode() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-049"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game, "BP17-049");
    game.resume_internal_card().unwrap();
    cast_repair(&mut game);
}
