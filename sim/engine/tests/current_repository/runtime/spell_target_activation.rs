#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

/// Answers printed questions, preferring members showing `wanted`.
fn answer(game: &mut Game, mut step: CardPlay, wanted: &str) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                let member = d
                    .members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(wanted))
                    .or_else(|| d.members.first())
                    .unwrap();
                Answer::Targets(vec![member.handle])
            }
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards.iter().take(1).map(|c| c.handle).collect(),
            )),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    finish_checks(game);
    step
}

fn position(printing: &str, cemetery: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
    initial["players"]["P1"]["zones"]["cemetery"] = cemetery;
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn desperados_shot(allies: Value) -> Value {
    let mut initial = in_hand("BP11-028");
    initial["players"]["P1"]["zones"]["field"] = allies;
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    resolved(answer(&mut game, play, "BP04-117"));
    game.projection(View::Referee).unwrap()
}

#[test]
fn desperados_shot_without_either_ally_only_deals_four_to_the_follower() {
    let view = desperados_shot(json!([]));
    assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(view["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn desperados_shot_with_both_allies_deals_six_to_the_follower_and_two_to_its_leader() {
    let view = desperados_shot(json!([
        {"id":"bunny","card":"BP11-019"}, {"id":"vehicle","card":"BP11-T01"}
    ]));
    assert_eq!(view["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn seed_shot_requires_a_plant_follower_for_leader_damage_and_draw() {
    for has_plant in [false, true] {
        let mut initial = in_hand("BP22-018");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
        if has_plant {
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"plant","card":"BP22-005"}]);
        }
        let mut game = game(&initial);
        let play = game.play_internal_card("source", actor()).unwrap();
        resolved(answer(&mut game, play, "BP04-117"));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(view["P2"]["leader"]["life"], if has_plant { 19_u8 } else { 20 });
        assert_eq!(view["P1"]["hand"], if has_plant { json!(["first"]) } else { json!([]) });
    }
}

#[test]
fn bp09_001_all_printings_search_an_elf_spell() {
    for printing in ["BP09-001", "BP09-SL01", "BP09-U01"] {
        let mut initial = position(printing, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"elf-spell","card":"BP01-023"},
            {"id":"plain","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let played = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, played, "");
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("fanfare");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(&mut game, step, "");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["elf-spell"]), "{printing}");
    }
}
