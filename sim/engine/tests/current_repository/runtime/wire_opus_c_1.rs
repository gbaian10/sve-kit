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

/// Answers printed questions: targets prefer `chosen` (else the first member), orders keep the offer.
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
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
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

fn evolve(game: &mut Game, sep: u8) {
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":sep}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "the printed evolution is legal");
}

fn luna_cemetery(fillers: usize) -> Value {
    let mut cards = vec![
        json!({"id":"cheap","card":"BP01-109"}),
        json!({"id":"costly","card":"BP02-087"}),
        json!({"id":"same-name","card":"BP17-073"}),
    ];
    cards.extend((0..fillers).map(|i| json!({"id":format!("filler{i}"),"card":"BP01-037"})));
    Value::Array(cards)
}

#[test]
fn bp17_073_all_printings_place_luna_doll_and_return_an_other_named_cheap_nightmare() {
    for printing in ["BP17-073", "BP17-SL17", "BP17-SP01", "BP17-U07"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["cemetery"] = luna_cemetery(7);
        let mut game = game(&initial);
        let entered = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(entered, CardPlay::Entered(_)), "{printing}");
        assert_eq!(drain(&mut game, ""), 1, "{printing}: Fanfare");
        let placed = game.projection(View::Referee).unwrap();
        let ex = placed["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1, "{printing}");
        assert_eq!(
            placed["objects"][ex[0].as_str().unwrap()]["name"],
            "ルナの人形",
            "{printing}"
        );
        // Ten cemetery cards: the act offers only the other-named cost-2 Nightmare follower.
        let act = game
            .activate_internal_card("source", activated("BP17-073"), actor())
            .unwrap();
        let CardPlay::Awaiting(q) = &act else {
            panic!("{printing}: cemetery target");
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("{printing}: cemetery target");
        };
        let offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
        assert_eq!(offered, ["BP01-109"], "{printing}");
        answer(&mut game, act, "BP01-109");
        let returned = game.projection(View::Referee).unwrap();
        assert_eq!(returned["P1"]["hand"], json!(["cheap"]), "{printing}");
        assert_eq!(returned["objects"]["source"]["acted"], true, "{printing}");
    }
}

#[test]
fn bp17_073_cannot_act_with_nine_cemetery_cards() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-073"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = luna_cemetery(6);
    let mut game = game(&initial);
    game.activate_internal_card("source", activated("BP17-073"), actor())
        .unwrap_err();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["acted"], false);
}

fn azurfleet(form: &str, sep: u8) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP20-057"},
        {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["sep"] = json!(sep);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game, sep);
    game
}

#[test]
fn bp20_058_evolution_pings_every_follower_and_its_own_ability_damage_pings_enemies_again() {
    for form in ["BP20-058", "BP20-SL18"] {
        let mut game = azurfleet(form, 0);
        // Evolution damages every follower including itself; that own-turn ability damage triggers line 2.
        assert_eq!(drain(&mut game, ""), 2, "{form}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 2_u8, "{form}");
        assert_eq!(view["objects"]["ally"]["damage"], 1_u8, "{form}");
        assert_eq!(view["objects"]["source"]["damage"], 1_u8, "{form}");
        assert_eq!(view["P2"]["leader"]["life"], 20_u8, "{form}");
    }
}

#[test]
fn bp20_058_super_evolution_also_hits_leaders_for_four_and_sets_its_defense_to_seven() {
    for form in ["BP20-058", "BP20-SL18"] {
        let mut game = azurfleet(form, 1);
        // Evolution and super evolution each ping every follower; both own-turn hits trigger line 2.
        assert_eq!(drain(&mut game, ""), 4, "{form}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 16_u8, "{form}");
        // Q2659: the current defense is 7 after the set, even though it was damaged before.
        assert_eq!(view["objects"]["source"]["hp"], 7_u8, "{form}");
        assert_eq!(view["objects"]["source"]["damage"], 2_u8, "{form}");
    }
}

#[test]
fn bp18_078_evolution_places_a_cheap_nightmare_or_tokyo_card_in_ex_at_two_less_this_turn() {
    for printing in ["BP18-078", "BP18-SL18", "BP18-U05"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-077"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"tokyo","card":"BP18-003"},
            {"id":"big","card":"BP02-087"},
            {"id":"neutral","card":"BP03-114"},
            {"id":"pick","card":"BP01-112"},
            {"id":"big-tokyo","card":"BP18-020"},
            {"id":"tail","card":"BP01-037"}
        ]);
        // Another cost-2 card already in EX keeps its printed cost.
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"other","card":"BP01-112"}]);
        let mut game = game(&initial);
        evolve(&mut game, 0);
        finish_checks(&mut game);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("{printing}: evolve trigger");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        let CardPlay::Awaiting(pick) = &step else {
            panic!("{printing}: Look choice");
        };
        let InputDomain::Targets(d) = &pick.domain else {
            panic!("{printing}: Look choice");
        };
        let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
        offered.sort();
        assert_eq!(offered, ["BP01-112", "BP18-003"], "{printing}");
        answer(&mut game, step, "BP01-112");
        let placed = game.projection(View::Referee).unwrap();
        assert_eq!(placed["P1"]["ex"], json!(["other", "pick"]), "{printing}");
        assert_eq!(placed["P1"]["deck"][0], "tail", "{printing}: the rest go to the bottom");
        let pp = placed["P1"]["pp"]["current"].as_u64().unwrap();
        let entered = game.play_internal_card("pick", actor()).unwrap();
        assert!(matches!(entered, CardPlay::Entered(_)), "{printing}");
        drain(&mut game, "");
        let paid = game.projection(View::Referee).unwrap();
        assert_eq!(paid["P1"]["pp"]["current"], pp, "{printing}: cost 2 - 2");
        let other = game.play_internal_card("other", actor()).unwrap();
        assert!(matches!(other, CardPlay::Entered(_)), "{printing}");
        drain(&mut game, "");
        let full = game.projection(View::Referee).unwrap();
        assert_eq!(full["P1"]["pp"]["current"], pp.saturating_sub(2), "{printing}: the other EX card pays 2");
    }
}

#[test]
fn bp18_078_super_evolution_also_places_the_evening_rest_token() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-077"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP18-078"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"tail","card":"BP01-037"}]);
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    let mut game = game(&initial);
    evolve(&mut game, 1);
    assert_eq!(drain(&mut game, ""), 2);
    let view = game.projection(View::Referee).unwrap();
    let names: Vec<_> = view["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| view["objects"][id.as_str().unwrap()]["name"].clone())
        .collect();
    assert_eq!(names, [json!("宵待ちの休息")]);
}
