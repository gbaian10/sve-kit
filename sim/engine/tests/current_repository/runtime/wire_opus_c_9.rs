#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers printed questions: targets prefer `chosen` (else the first member).
fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) {
    while let CardPlay::Awaiting(q) = step {
        let reply = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(chosen))
                    .or_else(|| d.members.first())
                    .map(|m| m.handle)
                    .into_iter()
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending printed trigger in offer order; returns how many resolved.
fn drain(game: &mut Game, chosen: &str) -> usize {
    let mut resolved = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step, chosen);
        resolved = resolved.saturating_add(1);
    }
    resolved
}

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

/// Opens the next pending trigger and returns the offered hand cards of its first choice.
fn offered_choice(game: &mut Game) -> (CardPlay, Vec<String>) {
    finish_checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger");
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    let CardPlay::Awaiting(pick) = &step else {
        panic!("hand choice");
    };
    let InputDomain::Targets(d) = &pick.domain else {
        panic!("hand choice: {:?}", pick.domain);
    };
    let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
    offered.sort();
    (step, offered)
}

fn shirou(form: &str, sep: u8) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["sep"] = json!(sep);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-097"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"tokyo","card":"BP18-003"},
        {"id":"altar","card":"BP04-115"},
        {"id":"big","card":"BP18-024"},
        {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP13-019"}]);
    let mut game = game(&initial);
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":sep}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "{form}: the printed evolution");
    game
}

#[test]
fn bp18_098_all_printings_put_a_cheap_tokyo_follower_or_bishop_amulet_from_hand() {
    for form in ["BP18-098", "BP18-SL23", "BP18-U06"] {
        let mut game = shirou(form, 0);
        let (step, offered) = offered_choice(&mut game);
        // The original-cost-4 Tokyo follower and the non-Tokyo follower are not offered.
        assert_eq!(offered, ["BP04-115", "BP18-003"], "{form}");
        answer(&mut game, step, "BP04-115");
        drain(&mut game, "");
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"].as_array().unwrap().contains(&json!("altar")),
            "{form}: {}",
            view["P1"]
        );
        assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 3, "{form}");
    }
}

#[test]
fn bp18_098_super_evolution_token_heals_and_the_heal_pings_an_enemy() {
    let mut game = shirou("BP18-098", 1);
    // Both evolution triggers; the hand choice takes the Tokyo follower.
    drain(&mut game, "BP18-003");
    let placed = game.projection(View::Referee).unwrap();
    let ex = placed["P1"]["ex"].as_array().unwrap().clone();
    assert_eq!(ex.len(), 1, "{}", placed["P1"]);
    let token = ex[0].as_str().unwrap().to_owned();
    assert_eq!(placed["objects"][token.as_str()]["name"], "善行の確信");
    let cast = game.play_internal_card(&token, actor()).unwrap();
    answer(&mut game, cast, "");
    // The heal triggers line 1: two damage to the enemy follower.
    assert_eq!(drain(&mut game, "BP13-019"), 1);
    let view = game.projection(View::Referee).unwrap();
    // Two 透京 followers on the field: the source and the placed one.
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert_eq!(view["P1"]["leader"]["life"], 21_u8);
    assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn bp13_090_all_printings_banish_a_follower_and_put_a_cheap_bishop_from_hand_once_per_turn() {
    for printing in ["BP13-090", "BP13-SL20"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":printing},
            {"id":"snake","card":"BP01-146"},
            {"id":"altar","card":"BP04-115"},
            {"id":"guard","card":"BP01-134"},
            {"id":"commander","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"sun","card":"BP01-149"},
            {"id":"spare","card":"BP01-149"}
        ]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"old0","card":"BP04-115"},
            {"id":"old1","card":"BP04-115"}
        ]);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let entered = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, entered, "");
        drain(&mut game, "BP01-037");
        let fanfare = game.projection(View::Referee).unwrap();
        assert_eq!(fanfare["P2"]["banish"], json!(["enemy"]), "{printing}");
        assert_eq!(fanfare["P2"]["leader"]["life"], 17_u8, "{printing}");
        assert_eq!(fanfare["P1"]["leader"]["life"], 13_u8, "{printing}");
        assert_eq!(fanfare["P1"]["banish"].as_array().unwrap().len(), 2, "{printing}");
        let mut act = game
            .activate_internal_card("source", activated("BP13-090"), actor())
            .unwrap();
        // One of the two field amulets pays first.
        while let CardPlay::Awaiting(q) = &act
            && let InputDomain::CostMaterials(d) = &q.domain
        {
            assert_eq!(d.members.len(), 2, "{printing}: both amulets");
            act = game
                .answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![d.members[0].handle]))
                .unwrap();
        }
        let CardPlay::Awaiting(pick) = &act else {
            panic!("{printing}: hand choice after paying the amulet");
        };
        let InputDomain::Targets(d) = &pick.domain else {
            panic!("{printing}: hand choice {:?}", pick.domain);
        };
        let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
        offered.sort();
        // Bishop followers or amulets of original cost 3 or less; the cost-5 Royal follower is not.
        assert_eq!(offered, ["BP01-134", "BP01-146", "BP04-115"], "{printing}");
        answer(&mut game, act, "BP01-146");
        drain(&mut game, "");
        let view = game.projection(View::Referee).unwrap();
        let paid = ["sun", "spare"]
            .iter()
            .filter(|id| view["P1"]["cemetery"].as_array().unwrap().contains(&json!(id)))
            .count();
        assert_eq!(paid, 1, "{printing}: one amulet paid");
        assert!(view["P1"]["field"].as_array().unwrap().contains(&json!("snake")), "{printing}");
        // The other amulet could still pay, so only the once-per-turn limit rejects a second use.
        game.activate_internal_card("source", activated("BP13-090"), actor())
            .unwrap_err();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["field"], view["P1"]["field"], "{printing}: nothing paid");
    }
}
