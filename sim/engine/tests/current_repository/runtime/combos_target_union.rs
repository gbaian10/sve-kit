#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-target-union",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial
}

/// Answers every question, picking target candidates by printed card number, and returns the
/// sorted candidate numbers of each target question in order.
fn drive(
    game: &mut Game,
    mut step: Option<CardPlay>,
    picks: &[&str],
    modes: &[u64],
) -> Vec<Vec<String>> {
    let mut offered = Vec::new();
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => {
                    let mut cards: Vec<String> = d
                        .members
                        .iter()
                        .map(|m| m.card.clone().unwrap_or_else(|| "leader".to_owned()))
                        .collect();
                    cards.sort();
                    offered.push(cards);
                    let maximum = d
                        .cardinality
                        .maximum
                        .map_or(usize::MAX, |n| usize::try_from(n.0).unwrap());
                    Answer::Targets(
                        d.members
                            .iter()
                            .filter(|m| m.card.as_deref().is_some_and(|c| picks.contains(&c)))
                            .take(maximum)
                            .map(|m| m.handle)
                            .collect(),
                    )
                }
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Modes { .. } => {
                    Answer::Modes(modes.iter().copied().map(ModeId).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Quick { .. } => Answer::Quick(None),
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
            return offered;
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

fn play(game: &mut Game, id: &str, picks: &[&str], modes: &[u64]) -> Vec<Vec<String>> {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    drive(game, Some(step), picks, modes)
}

fn evolve(game: &mut Game, super_evolution: bool, picks: &[&str]) -> Vec<Vec<String>> {
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form",
            "pay":{"pp":1_u8,"sep":u8::from(super_evolution)}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "the real base evolves");
    drive(game, None, picks, &[])
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn numbers(cards: &[&str]) -> Vec<String> {
    cards.iter().map(|&card| card.to_owned()).collect()
}

// One real-card scenario per printed card; union boundaries live in the synthetic
// `internal_ir` target union cases.
#[test]
fn bp03_105_targets_another_storm_or_ward_follower() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-105"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"storm","card":"BP01-048"},
        {"id":"ward","card":"BP01-159"},
        {"id":"plain","card":"BP01-037"}
    ]);
    let mut game = loaded(&initial);
    let offered = play(&mut game, "source", &["BP01-159"], &[]);
    assert_eq!(offered, vec![numbers(&["BP01-048", "BP01-159"])]);
    let view = state(&game);
    assert_eq!(view["objects"]["ward"]["power"], 1_u8);
    assert_eq!(view["objects"]["source"]["power"], 2_u8);
}

#[test]
fn bp03_106_activation_pays_and_buffs_a_storm_or_ward_follower() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP03-106"},
        {"id":"storm","card":"BP01-048"},
        {"id":"ward","card":"BP01-159"},
        {"id":"plain","card":"BP01-037"}
    ]);
    let mut game = loaded(&initial);
    let ability = game.internal_activated_sources("source").unwrap()[0].definition;
    let step = game.activate_internal_card("source", ability, PLAYER).unwrap();
    let offered = drive(&mut game, Some(step), &["BP01-048"], &[]);
    assert_eq!(offered, vec![numbers(&["BP01-048", "BP01-159"])]);
    let view = state(&game);
    assert_eq!(view["objects"]["storm"]["power"], 4_u8);
    assert_eq!(view["objects"]["storm"]["hp"], 4_u8);
    assert_eq!(view["objects"]["source"]["acted"], true);
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp04_128_banishes_a_low_power_or_one_health_enemy() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP04-127"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-128"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"weak","card":"BP01-159"},
        {"id":"hurt","card":"BP01-037","state":{"hp":1_u8,"max_hp":5_u8}},
        {"id":"big","card":"BP01-048"}
    ]);
    let mut game = loaded(&initial);
    let offered = evolve(&mut game, false, &["BP01-037"]);
    assert_eq!(offered, vec![numbers(&["BP01-037", "BP01-159"])]);
    let view = state(&game);
    assert_eq!(view["P2"]["banish"], json!(["hurt"]));
    assert_eq!(view["P2"]["cemetery"], json!([]));
}

#[test]
fn bp08_058_damages_by_the_power_of_an_ex_follower() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP08-058"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-048"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"stored","card":"BP01-158"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}]);
    let mut game = loaded(&initial);
    let offered = play(&mut game, "source", &["BP01-037", "BP01-158"], &[]);
    assert_eq!(
        offered,
        vec![
            numbers(&["BP01-037"]),
            numbers(&["BP01-048", "BP01-158"])
        ]
    );
    let view = state(&game);
    assert_eq!(view["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp10_116_excludes_only_itself_from_the_fallen_angel_branch() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP10-115"},
        {"id":"angel","card":"BP01-159"},
        {"id":"fallen","card":"BP03-119"},
        {"id":"plain","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-116"}]);
    let mut game = loaded(&initial);
    let offered = evolve(&mut game, false, &["BP03-119"]);
    assert_eq!(offered, vec![numbers(&["BP01-159", "BP03-119"])]);
    let view = state(&game);
    assert_eq!(view["objects"]["fallen"]["power"], 3_u8);
    assert_eq!(view["objects"]["fallen"]["hp"], 3_u8);
    assert_eq!(view["objects"]["source"]["power"], 3_u8);
}

#[test]
fn bp16_098_super_evolution_moves_a_faith_spell_or_amulet_and_discounts_it() {
    let mut initial = position();
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-097"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-098"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"spell","card":"BP11-101"},
        {"id":"amulet","card":"BP01-133"},
        {"id":"expensive","card":"BP01-131"},
        {"id":"follower","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = loaded(&initial);
    let offered = evolve(&mut game, true, &["BP01-037", "BP11-101"]);
    assert!(
        offered.contains(&numbers(&["BP01-133", "BP11-101"])),
        "only the faith spell and amulet costing four or less are offered: {offered:?}"
    );
    let view = state(&game);
    assert_eq!(view["P1"]["ex"], json!(["spell"]));
    assert_eq!(view["objects"]["enemy"]["power"], 0_u8);
    assert_eq!(view["objects"]["enemy"]["acted"], true);
    let before = view["P1"]["pp"]["current"].clone();
    play(&mut game, "spell", &["BP01-037"], &[]);
    let after = state(&game);
    assert_eq!(after["P1"]["ex"], json!([]), "the discounted spell was played");
    assert_eq!(
        after["P1"]["pp"]["current"], before,
        "the printed cost of four drops to zero"
    );
}

#[test]
fn bp21_007_buffs_fairy_tokens_on_the_field_and_in_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP21-007"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"near","card":"BP01-T03"},
        {"id":"plain","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"far","card":"BP01-T03"}]);
    let mut game = loaded(&initial);
    let offered = play(&mut game, "source", &["BP01-T03"], &[1]);
    assert_eq!(offered, vec![numbers(&["BP01-T03", "BP01-T03"])]);
    let view = state(&game);
    for id in ["near", "far"] {
        assert_eq!(view["objects"][id]["power"], 2_u8);
        assert_eq!(view["objects"][id]["hp"], 2_u8);
    }
}

#[test]
fn csd01_007_puts_an_uma_amulet_from_the_cemetery_onto_the_field() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CSD01-007"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"follower","card":"CP01-001"},
        {"id":"amulet","card":"CP01-008"},
        {"id":"expensive","card":"BP22-117"},
        {"id":"other","card":"BP01-159"}
    ]);
    let mut game = loaded(&initial);
    let offered = play(&mut game, "source", &["CP01-008"], &[]);
    assert_eq!(offered, vec![numbers(&["CP01-001", "CP01-008"])]);
    let view = state(&game);
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("amulet")),
        "the chosen amulet entered: {}",
        view["P1"]["field"]
    );
    assert_eq!(view["P1"]["pp"]["current"], 4_u8);
}
