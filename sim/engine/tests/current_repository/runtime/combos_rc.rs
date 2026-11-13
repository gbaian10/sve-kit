#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card probes."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-rc",
    )
    .unwrap()
}

fn source(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial
}

fn settle(game: &mut Game, mut step: Option<CardPlay>, modes: &[ModeId]) {
    for _ in 0_u8..80_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected probe decision"),
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
            panic!("expected pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("probe did not settle");
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", PLAYER).unwrap();
    settle(game, Some(step), &[ModeId(1)]);
}
fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp02_106_uses_both_mode_pairs_and_preserves_the_other_actions() {
    for first_pair in [true, false] {
        let mut initial = source("BP02-106", "hand");
        initial["players"]["P1"]["leader"]["life"] = json!(17_u8);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"discarded","card":"BP01-171"}]);
        let mut game = loaded(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        let modes = if first_pair {
            vec![ModeId(1), ModeId(4)]
        } else {
            vec![ModeId(2), ModeId(3)]
        };
        settle(&mut game, Some(step), &modes);
        let view = state(&game);
        assert_eq!(view["P1"]["pp"]["current"], 5_u8);
        if first_pair {
            assert_eq!(view["P1"]["ep"], 1_u8);
            assert_eq!(view["P1"]["leader"]["life"], 17_u8);
            assert_eq!(view["P1"]["hand"], json!([]));
            assert_eq!(view["P2"]["hand"], json!([]));
            assert_eq!(view["P2"]["cemetery"], json!(["discarded"]));
        } else {
            assert_eq!(view["P1"]["ep"], 0_u8);
            assert_eq!(view["P1"]["leader"]["life"], 20_u8);
            assert_eq!(view["P1"]["hand"], json!(["drawn"]));
            assert_eq!(view["P2"]["hand"], json!(["discarded"]));
            assert_eq!(view["P2"]["cemetery"], json!([]));
        }
    }
}

#[test]
fn bp04_100_real_evolution_requires_life_and_a_follower_target() {
    for (life, has_target) in [(20_u8, true), (4_u8, true), (20_u8, false)] {
        let mut initial = source("BP04-099", "field");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-100"}]);
        if has_target {
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}]);
        }
        let mut game = loaded(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        settle(&mut game, None, &[]);
        let view = state(&game);
        let damage = if life >= 5_u8 && has_target {
            4_u8
        } else {
            0_u8
        };
        assert_eq!(view["P1"]["leader"]["life"], life - damage);
        assert_eq!(view["P2"]["leader"]["life"], 20_u8 - damage);
        if has_target {
            assert_eq!(view["objects"]["enemy"]["damage"], damage);
        }
    }
}

#[test]
fn bp06_097_fanfare_requires_another_ward_and_a_legal_target() {
    for (other_ward, has_target) in [(true, true), (false, true), (true, false)] {
        let mut initial = source("BP06-097", "hand");
        if other_ward {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ward","card":"BP01-159"}]);
        }
        if has_target {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        }
        let mut game = loaded(&initial);
        play(&mut game);
        let view = state(&game);
        if has_target {
            assert_eq!(
                view["objects"]["enemy"]["damage"],
                if other_ward { 3_u8 } else { 0_u8 }
            );
        }
        assert_eq!(view["P1"]["pp"]["current"], 5_u8);
    }
}
