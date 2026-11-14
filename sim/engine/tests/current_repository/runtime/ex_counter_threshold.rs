#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };
const PASSION: &str = "情熱カウンター";

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-counter-threshold",
    )
    .unwrap()
}

/// `source` stands on P1's field; each EX card holds the given passion counters.
fn position(card: &str, own: &[u8], opponent: &[u8]) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    let stored = |prefix: &str, counts: &[u8]| {
        counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                json!({
                    "id": format!("{prefix}{index}"),
                    "card": "BP21-057",
                    "state": {"counters": {PASSION: count}},
                })
            })
            .collect::<Vec<_>>()
    };
    initial["players"]["P1"]["zones"]["ex"] = json!(stored("own", own));
    initial["players"]["P2"]["zones"]["ex"] = json!(stored("opp", opponent));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":12_u8,"max_hp":12_u8}}]);
    initial
}

fn activate(game: &mut Game, card: &str, line: u32) {
    let program = &selection_with_rules().current[card].program;
    let ability = program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    settle(game, Some(step));
}

/// Resolves pending abilities and declines every Quick window until nothing is left.
fn drain(game: &mut Game) {
    settle(game, None);
    while let Some(q) = game.internal_quick_choice().unwrap() {
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        settle(game, Some(step));
    }
}

fn attack(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    drain(game);
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    settle(game, Some(step));
}

/// Picks the opposing follower for targets (else the only form to evolve into) and resolves every pending ability in order.
fn settle(game: &mut Game, mut step: Option<CardPlay>) {
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .find(|m| m.card.as_deref() == Some("BP01-037"))
                        .or_else(|| d.members.first())
                        .map(|m| m.handle)
                        .into_iter()
                        .collect(),
                ),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { .. } => Answer::Quick(None),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn passion(view: &Value, id: &str) -> u64 {
    view["objects"][id]["counters"][PASSION]
        .as_u64()
        .unwrap_or_default()
}

fn hp(view: &Value, id: &str) -> u64 {
    view["objects"][id]["hp"].as_u64().unwrap()
}

#[test]
fn bp21_069_needs_one_own_ex_card_at_the_threshold() {
    // Neither two own cards below 4 nor the opponent's EX count; one own card at 10 does both.
    for (own, opponent, life, bonus) in [
        (&[3_u8, 2][..], &[10_u8][..], 10_u8, 0),
        (&[4, 10], &[], 12, 6),
    ] {
        let mut game = loaded(&position("BP21-069", own, opponent));
        let before = hp(&state(&game), "source");
        activate(&mut game, "BP21-069", 3);
        let view = state(&game);
        assert_eq!(view["P1"]["leader"]["life"], life, "{own:?}");
        assert_eq!(hp(&view, "source"), before + bonus, "{own:?}");
    }
}

#[test]
fn bp21_065_damages_its_target_at_four_counters() {
    let mut game = loaded(&position("BP21-065", &[4], &[]));
    activate(&mut game, "BP21-065", 3);
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp21_060_deals_four_instead_of_two_when_evolving() {
    let mut initial = position("BP21-059", &[4], &[]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-060"}]);
    let mut game = loaded(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    drain(&mut game);
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp21_061_attack_raises_max_pp_and_grows_at_ten() {
    let mut initial = position("BP21-061", &[10], &[]);
    // Maximum PP stops at 10, so start below it.
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    let mut game = loaded(&initial);
    let before = state(&game);
    attack(&mut game);
    let view = state(&game);
    assert_eq!(
        view["P1"]["pp"]["max"].as_u64().unwrap(),
        before["P1"]["pp"]["max"].as_u64().unwrap() + 1
    );
    assert_eq!(
        view["objects"]["source"]["max_hp"].as_u64().unwrap(),
        before["objects"]["source"]["max_hp"].as_u64().unwrap() + 6
    );
}

#[test]
fn bp21_063_attack_evolves_itself_at_four() {
    let mut initial = position("BP21-063", &[4], &[]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-064"}]);
    let mut game = loaded(&initial);
    attack(&mut game);
    // The evolved form dies in combat and its card returns to the evolve deck, so the damage
    // it dealt is the evidence: 3 from the form instead of the base's 1.
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn bp21_057_counts_academy_entries_only_from_ex() {
    // Q2736: the EX copy watches each academy follower that enters P1's field.
    let mut initial = position("BP01-037", &[], &[]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"joe","card":"BP21-057","state":{"counters":{PASSION: 8_u8}}}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"academy","card":"BP21-055"},
        {"id":"plain","card":"BP01-037"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, "academy");
    assert_eq!(passion(&state(&game), "joe"), 9);
    play(&mut game, "plain");
    assert_eq!(passion(&state(&game), "joe"), 9, "a non-academy follower");
}

#[test]
fn bp21_057_on_field_ignores_entries() {
    let mut initial = position("BP21-057", &[], &[]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"academy","card":"BP21-055"}]);
    let mut game = loaded(&initial);
    play(&mut game, "academy");
    assert_eq!(passion(&state(&game), "source"), 0);
}

#[test]
fn bp21_057_keeps_ex_counters_when_played_to_the_field() {
    // Q2737: counters placed in EX stay on the card that enters from EX.
    let mut initial = position("BP01-037", &[], &[]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"source","card":"BP21-057","state":{"counters":{PASSION: 10_u8}}}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, "source");
    let entered = state(&game);
    assert_eq!(entered["P1"]["field"], json!(["source"]));
    assert_eq!(passion(&entered, "source"), 10);
}

#[test]
fn bp21_057_reads_itself_or_an_ex_card() {
    // (own counters, EX counters) -> damage dealt; Joe gains 6 hp either way.
    for (own, stored, damage) in [
        (0_u8, &[9_u8][..], false),
        (10, &[][..], true),
        (0, &[10][..], true),
    ] {
        let mut initial = position("BP21-057", stored, &[]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP21-057","state":{"counters":{PASSION: own}}}]);
        initial["players"]["P2"]["leader"]["life"] = json!(20_u8);
        let mut game = loaded(&initial);
        let before = hp(&state(&game), "source");
        activate(&mut game, "BP21-057", 2);
        let view = state(&game);
        assert_eq!(
            view["P2"]["leader"]["life"],
            if damage { 15_u8 } else { 20_u8 },
            "{own} {stored:?}"
        );
        assert_eq!(hp(&view, "source"), before + 6);
    }
}

#[test]
fn bp21_057_moves_itself_from_hand_to_ex_as_the_effect() {
    // Q2738: the zero-cost ability works from the hand. The move is the effect, not a cost: with
    // a full EX the ability still resolves and Joe simply stays in the hand.
    for full in [false, true] {
        let mut initial = position("BP01-037", if full { &[0_u8; 5][..] } else { &[][..] }, &[]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP21-057"}]);
        let mut game = loaded(&initial);
        let before = state(&game);
        activate(&mut game, "BP21-057", 3);
        drain(&mut game);
        let view = state(&game);
        if full {
            assert_eq!(
                view["P1"]["hand"],
                json!(["source"]),
                "a full EX leaves Joe in hand"
            );
            assert_eq!(view["P1"]["ex"], before["P1"]["ex"]);
        } else {
            assert_eq!(view["P1"]["ex"], json!(["source"]));
            assert_eq!(view["P1"]["hand"], json!([]));
        }
        assert_eq!(view["P1"]["pp"]["current"], before["P1"]["pp"]["current"]);
    }
}
