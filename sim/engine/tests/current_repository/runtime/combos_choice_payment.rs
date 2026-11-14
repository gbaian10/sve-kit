#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::DistributionMember;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-choice-payment",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial
}

/// Plays `source`, choosing `modes`, accepting mode fees and picking candidates by card number.
fn play(game: &mut Game, modes: &[u64], picks: &[&str]) {
    let pick = |members: &[DistributionMember], count: usize| {
        members
            .iter()
            .filter(|m| m.card.as_deref().is_some_and(|c| picks.contains(&c)))
            .take(count)
            .map(|m| m.handle)
            .collect::<Vec<_>>()
    };
    let mut step = Some(game.play_internal_card("source", PLAYER).unwrap());
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(pick(
                    &d.members,
                    usize::try_from(d.cardinality.minimum.0.max(1)).unwrap(),
                )),
                InputDomain::CostMaterials(d) => Answer::CostMaterials(pick(
                    &d.members,
                    usize::try_from(d.cardinality.minimum.0).unwrap(),
                )),
                InputDomain::Modes { .. } => {
                    Answer::Modes(modes.iter().copied().map(ModeId).collect())
                }
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
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

#[test]
fn bp22_062_pays_five_for_the_second_mode_and_grants_assail() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-062"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"roar","card":"BP22-067"},
        {"id":"ignis","card":"BP22-056"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[1, 2], &["BP22-067", "BP22-056"]);
    let view = state(&game);
    assert_eq!(view["P1"]["ex"], json!(["roar"]));
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("ignis"))
    );
    assert_eq!(view["P1"]["pp"]["current"], 4_u8);
    assert!(
        view["objects"]["ignis"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
}

fn naginata(number: &str, modes: &[u64]) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":number},
        {"id":"spare","card":"BP01-048"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"one","card":"BP01-146"},
        {"id":"two","card":"BP03-033"},
        {"id":"three","card":"BP01-048"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = loaded(&initial);
    play(
        &mut game,
        modes,
        &["BP01-146", "BP03-033", "BP01-037", "BP01-048"],
    );
    state(&game)
}

#[test]
fn cp02_020_banishes_two_cost_one_cards_then_resolves_two_modes() {
    let view = naginata("CP02-020", &[1, 2]);
    let mut banished: Vec<String> = serde_json::from_value(view["P1"]["banish"].clone()).unwrap();
    banished.sort();
    assert_eq!(banished, ["one", "two"]);
    assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn cp02_sp04a_draws_and_discards_beside_leader_damage() {
    let view = naginata("CP02-SP04a", &[2, 3]);
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert_eq!(view["P2"]["field"], json!(["enemy"]));
    assert_eq!(view["P1"]["deck"], json!([]));
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 1);
}
