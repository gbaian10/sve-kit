#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-171"},
            {"id":format!("{seat}-second"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            let member = domain
                .members
                .iter()
                .find(|m| m.card.as_deref() == Some("BP01-037"))
                .unwrap_or(&domain.members[0]);
            Answer::Targets(vec![member.handle])
        } else if matches!(&q.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    checks(game);
}

fn trigger(game: &mut Game, card: &str) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("expected a pending ability")
    };
    let chosen = options.iter().find(|option| option.card == card).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish(game, step);
    checks(game);
}

#[test]
fn bp13_076_fixes_necrocharge_at_resolution_start_before_returning_the_blade() {
    for grave_count in [19_u8, 20, 21] {
        let mut initial = position("BP13-076");
        let mut grave = vec![json!({"id":"blade","card":"BP13-081"})];
        grave.extend(
            (1_u8..grave_count).map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-171"})),
        );
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game, "BP13-076");
        let returned = game.projection(View::Referee).unwrap();
        assert_eq!(returned["P1"]["ex"], json!(["blade"]));
        assert_eq!(returned["objects"]["source"]["power"], 2_u8);
        play(&mut game, "blade");
        let played = game.projection(View::Referee).unwrap();
        assert_eq!(
            played["P1"]["pp"]["current"],
            if grave_count >= 20 { 8_u8 } else { 6 }
        );
        assert_eq!(played["objects"]["enemy"]["hp"], 1_u8);
        assert_eq!(
            played["P2"]["leader"]["life"],
            if grave_count == 21 { 16_u8 } else { 20 }
        );
    }
}

#[test]
fn bp13_020_counts_the_follower_just_destroyed_and_requires_ten_levion_to_stand() {
    for count in [9_u8, 10] {
        let mut initial = position("BP13-020");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-020"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..count)
                .map(|i| json!({"id":format!("levion-{i}"),"card":"BP13-032"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game, "BP13-020");
        let attacking = game.projection(View::Referee).unwrap();
        assert_eq!(attacking["P2"]["field"], json!([]));
        assert_eq!(attacking["objects"]["source"]["power"], 5_u8);
        assert_eq!(attacking["objects"]["source"]["hp"], 7_u8);
        for _ in 0..2_u8 {
            let Some(q) = game.internal_quick_choice().unwrap() else {
                break;
            };
            let step = game
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(&mut game, step);
            checks(&mut game);
        }
        let ability = selection().current["BP13-020"]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let result = game.activate_internal_card("source", ability, actor());
        if count == 10 {
            finish(&mut game, result.unwrap());
            let stood = game.projection(View::Referee).unwrap();
            assert_eq!(stood["objects"]["source"]["acted"], false);
            assert_eq!(stood["P1"]["pp"]["current"], 8_u8);
        } else {
            result.unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                10_u8
            );
        }
    }
}
