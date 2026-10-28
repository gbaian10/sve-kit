#![expect(
    clippy::panic,
    reason = "Malformed runtime questions mean the card scenario cannot be resolved."
)]

use super::*;

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn resolve_pending(game: &mut Game, card: &str, line: u64, target_card: Option<&str>) {
    let pending = game.internal_pending_choice().unwrap().unwrap_or_else(|| {
        panic!(
            "missing pending choice; state: {:?}",
            game.projection(View::Referee)
        )
    });
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected a pending ability choice");
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap();
    if let Some(target_card) = target_card {
        let CardPlay::Awaiting(question) = play else {
            panic!("the ability must wait for its follower target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("the ability must ask for a follower target");
        };
        let target = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some(target_card))
            .unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![target.handle]),
        )
        .unwrap();
    } else if matches!(play, CardPlay::Awaiting(_)) {
        panic!("this ability has no target choice");
    } else {
        // Targetless abilities finish without a further card decision.
    }
    finish_checks(game);
}

fn attack(game: &mut Game, attacker: &str, target: &str) -> String {
    game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap()
        .outcome
}

fn pass_quick(game: &mut Game) {
    let question = match game.internal_decision(View::Referee) {
        Some(question) if matches!(question.domain, InputDomain::Quick { .. }) => Some(question),
        Some(_) => return,
        None if game.internal_pending_choice().unwrap().is_some() => return,
        None => game.internal_quick_choice().unwrap(),
    };
    let Some(question) = question else { return };
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish_checks(game);
}

fn attack_and_pass(game: &mut Game, attacker: &str, target: &str, trigger: Option<(&str, u64)>) {
    assert_eq!(
        attack(game, attacker, target),
        "resolved",
        "the follower attack must enter combat"
    );
    finish_checks(game);
    pass_quick(game);
    if let Some((card, line)) = trigger {
        resolve_pending(game, card, line, None);
    }
    pass_quick(game);
}

fn pass_end_phase(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    loop {
        finish_checks(game);
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

fn play_and_resolve_fanfare(game: &mut Game, source: &str, card: &str, line: u64) {
    assert!(
        matches!(
            game.play_internal_card(source, PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower must enter before its fanfare resolves"
    );
    resolve_pending(game, card, line, None);
}

#[test]
fn bp05_075_attack_time_and_fanfare_damage_all_leaders() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP05-075"}]);
    let mut fanfare_game = game(&initial);
    play_and_resolve_fanfare(&mut fanfare_game, "source", "BP05-075", 2);
    let state = fanfare_game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u64);
    assert_eq!(state["P2"]["leader"]["life"], 19_u64);

    let mut attack_position = draw_position();
    attack_position["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-075","state":{"acted":false,"entered_this_turn":false}}]);
    attack_position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&attack_position);
    attack_and_pass(&mut game, "source", "target", Some(("BP05-075", 1)));
    let attack_state = game.projection(View::Referee).unwrap();
    assert_eq!(attack_state["P1"]["leader"]["life"], 19_u64);
    assert_eq!(attack_state["P2"]["leader"]["life"], 19_u64);
}

#[test]
fn bp05_080_fanfare_damages_its_controller() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP05-080"}]);
    let mut fanfare_game = game(&initial);
    play_and_resolve_fanfare(&mut fanfare_game, "source", "BP05-080", 2);
    assert_eq!(
        fanfare_game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        19_u64
    );
}

#[test]
fn bp05_099_bane_destroys_a_follower_and_lastword_damages_opposing_leader() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"bane","card":"BP05-099","state":{"acted":false,"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-037","state":{"acted":true}}]);
    let mut bane_game = game(&initial);
    attack_and_pass(&mut bane_game, "bane", "target", None);
    assert_eq!(
        bane_game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["target"])
    );

    let mut second_initial = draw_position();
    second_initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP05-099","state":{"acted":false,"entered_this_turn":false}}]);
    second_initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"epitaph","card":"BP05-099","state":{"acted":true,"damage":3_u8}}]);
    let mut game = game(&second_initial);
    attack_and_pass(&mut game, "attacker", "epitaph", Some(("BP05-099", 2)));
    resolve_pending(&mut game, "BP05-099", 2, None);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u64
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u64
    );
}

#[test]
fn bp05_t02_damages_opposing_leaders_at_its_controllers_main_start() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"artifact","card":"BP05-T02"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    pass_end_phase(&mut game);
    resolve_pending(&mut game, "BP05-T02", 1, None);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u64
    );
}

#[test]
fn bp05_093_damages_a_follower_at_the_opponents_main_start() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-093"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    pass_end_phase(&mut game);
    resolve_pending(&mut game, "BP05-093", 1, Some("BP01-037"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["target"]["hp"],
        4_u64
    );
}

#[test]
fn bp05_059_activates_once_to_damage_its_controllers_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP05-059"},
        {"id":"ally","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the ability waits for a friendly follower");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the ability chooses a friendly follower");
    };
    let ally = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![ally.handle]),
    )
    .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["hp"],
        4_u64
    );
    game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
}

#[test]
fn bp05_111_ward_protects_its_leader() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP05-111","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(attack(&mut game, "attacker", "P1.leader"), "cannot-attack");
}

#[test]
fn bp05_t05_ward_protects_its_leader() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP05-T05","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(attack(&mut game, "attacker", "P1.leader"), "cannot-attack");
}

#[test]
fn bp05_097_restores_one_life_at_the_opponents_main_start() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-097"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    pass_end_phase(&mut game);
    resolve_pending(&mut game, "BP05-097", 1, None);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        11_u64
    );
}
#[test]
fn bp05_t01_restores_two_life_at_its_controllers_main_start() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"artifact","card":"BP05-T01"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    pass_end_phase(&mut game);
    resolve_pending(&mut game, "BP05-T01", 1, None);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_u64
    );
}
