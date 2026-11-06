#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the printed Orchis scenarios."
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

fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) {
    while let CardPlay::Awaiting(q) = step {
        let response = match &q.domain {
            InputDomain::Targets(d) => {
                let member = d
                    .members
                    .iter()
                    .find(|m| {
                        if chosen.is_empty() {
                            m.card.is_none()
                        } else {
                            m.card.as_deref() == Some(chosen)
                        }
                    })
                    .or_else(|| d.members.first());
                Answer::Targets(member.map_or_else(Vec::new, |member| vec![member.handle]))
            }
            InputDomain::TriggerQuota(d) => Answer::TriggerQuota(d.candidates[0].handle),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Pending(_) => return,
            _ => panic!("unexpected printed choice: {:?}", q.domain),
        };
        step = game
            .answer_internal_card(q.actor, q.token, response)
            .unwrap();
    }
    finish_checks(game);
}

fn drain(game: &mut Game, chosen: &str) -> usize {
    let mut count = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let response = match &q.domain {
            InputDomain::Pending(options) => {
                count = count.checked_add(1).unwrap();
                Answer::Pending(options[0].handle)
            }
            InputDomain::TriggerQuota(d) => Answer::TriggerQuota(d.candidates[0].handle),
            _ => panic!("unexpected pending choice: {:?}", q.domain),
        };
        let step = game
            .answer_internal_card(q.actor, q.token, response)
            .unwrap();
        answer(game, step, chosen);
    }
    count
}

fn orchis(form: &str, face: u8, with_lloyd: bool) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP08-002"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"grave0","card":"BP08-002"},
        {"id":"grave1","card":"BP08-002"},
        {"id":"grave2","card":"BP08-002"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"feast0","card":"BP16-086"},
        {"id":"feast1","card":"BP16-086"},
        {"id":"blaze","card":"BP01-089"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-037"},
        {"id":"draw1","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy0","card":"BP01-037"},
        {"id":"enemy1","card":"BP01-037"}
    ]);
    if with_lloyd {
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"lloyd","card":"BP08-T01"}]);
    }
    let mut game = game(&initial);
    let definition = selection().current["BP08-002"].program.card().abilities[usize::from(face)]
        .node
        .id;
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form",
            "definition":definition.0,"face":face,"pay":{"pp":if face == 0 { 1_u8 } else { 4_u8 }}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "{form}");
    assert_eq!(drain(&mut game, ""), 1, "{form}");
    game
}

fn dolls(game: &Game) -> Vec<String> {
    let view = game.projection(View::Referee).unwrap();
    view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == "操り人形")
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

fn named(game: &Game, zone: &str, name: &str) -> usize {
    let view = game.projection(View::Referee).unwrap();
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

/// Casts 魂の捕食 on an enemy follower and one own doll; returns the triggers it caused.
fn feast(game: &mut Game, id: &str, chosen: &str) -> usize {
    let step = game.play_internal_card(id, actor()).unwrap();
    answer(game, step, "BP08-T09");
    drain(game, chosen)
}

#[test]
fn bp08_003_resolve_face_makes_two_dolls_and_a_leaving_doll_places_lloyd_once_per_turn() {
    for form in ["BP08-003", "BP08-SL03"] {
        let mut game = orchis(form, 0, false);
        assert_eq!(dolls(&game).len(), 2, "{form}");
        assert_eq!(feast(&mut game, "feast0", ""), 1, "{form}");
        assert_eq!(dolls(&game).len(), 1, "{form}");
        assert_eq!(named(&game, "ex", "ロイド"), 1, "{form}");
        // Once per own turn: the second doll leaving adds nothing.
        assert_eq!(feast(&mut game, "feast1", ""), 0, "{form}");
        assert_eq!(dolls(&game).len(), 0, "{form}");
        assert_eq!(named(&game, "ex", "ロイド"), 1, "{form}");
    }
}

#[test]
fn bp08_003_revenge_face_makes_four_assailing_dolls_that_ping_when_they_leave() {
    for form in ["BP08-003", "BP08-SL03"] {
        let mut game = orchis(form, 1, false);
        let dolls = dolls(&game);
        assert_eq!(dolls.len(), 4, "{form}");
        let granted = game.projection(View::Referee).unwrap();
        for doll in &dolls {
            assert!(
                granted["objects"][doll.as_str()]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("指定攻撃")),
                "{form}: {}",
                granted["objects"][doll.as_str()]
            );
        }
        // Each leaving doll: two damage to the opponent leader or a chosen enemy follower.
        assert_eq!(feast(&mut game, "feast0", "BP01-037"), 1, "{form}");
        let first = game.projection(View::Referee).unwrap();
        assert_eq!(first["objects"]["enemy1"]["damage"], 2_u8, "{form}");
        assert_eq!(feast(&mut game, "feast1", ""), 1, "{form}");
        let second = game.projection(View::Referee).unwrap();
        assert_eq!(second["P2"]["leader"]["life"], 18_u8, "{form}");
        assert_eq!(named(&game, "ex", "ロイド"), 0, "{form}");
    }
}

#[test]
fn bp08_003_dolls_leaving_together_with_the_source_still_trigger_each_face() {
    // Q1055 / Q1254: the source and its dolls leave at once; 灼熱の嵐 hits every follower for 5.
    for (face, triggers, lloyds, life) in [(1, 4, 0, 12_u8), (0, 1, 1, 20_u8)] {
        let mut game = orchis("BP08-003", face, false);
        let step = game.play_internal_card("blaze", actor()).unwrap();
        answer(&mut game, step, "");
        assert_eq!(drain(&mut game, ""), triggers, "face {face}");
        assert_eq!(dolls(&game).len(), 0, "face {face}");
        assert_eq!(named(&game, "ex", "ロイド"), lloyds, "face {face}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], life, "face {face}");
    }
}

#[test]
fn bp08_003_resolve_face_ignores_dolls_leaving_on_the_opponents_turn() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP08-002","state":{"evolved":true,"evolved_with":"form"}},
        {"id":"doll0","card":"BP08-T09"},
        {"id":"doll1","card":"BP08-T09"}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"BP08-003"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"blaze","card":"BP01-089"}]);
    let mut game = game(&initial);
    let step = game
        .play_internal_card("blaze", PlayerId { seat: 2 })
        .unwrap();
    answer(&mut game, step, "");
    assert_eq!(drain(&mut game, ""), 0);
    assert_eq!(dolls(&game).len(), 0);
    assert_eq!(named(&game, "ex", "ロイド"), 0);
}

#[test]
fn named_reverse_evolutions_return_the_whole_form_when_a_spell_destroys_the_holder() {
    for (base, form, definition, pp) in [
        ("BP08-002", "BP08-003", 1, 4_u8),
        ("BP09-004", "BP09-005", 0, 1),
        ("BP09-018", "BP09-019", 1, 4),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"grave0","card":"BP08-002"}, {"id":"grave1","card":"BP08-002"}, {"id":"grave2","card":"BP08-002"}
        ]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"feast","card":"BP16-086"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let definition = selection().current[base].program.card().abilities[definition]
            .node
            .id;
        let outcome = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form", "definition":definition.0,"face":1_u8,"pay":{"pp":pp}})).unwrap();
        assert_eq!(outcome.outcome, "resolved");
        drain(&mut game, "");
        let step = game.play_internal_card("feast", actor()).unwrap();
        answer(&mut game, step, base);
        drain(&mut game, "");
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "{form}"
        );
        assert_eq!(state["P1"]["evolution"], json!([]), "{form}");
        assert_eq!(state["P1"]["evolve_deck"], json!(["form"]), "{form}");
        assert_eq!(state["objects"]["form"]["face"], 0_u8, "{form}");
        assert_eq!(state["objects"]["form"]["face_up"], true, "{form}");
    }
}

#[test]
fn bp08_003_revenge_grants_assail_to_lloyd_and_observes_its_field_alias_when_it_leaves() {
    let mut game = orchis("BP08-003", 1, true);
    assert_eq!(feast(&mut game, "feast0", ""), 1);
    let entry = game.play_internal_card("lloyd", actor()).unwrap();
    answer(&mut game, entry, "");
    assert_eq!(drain(&mut game, ""), 1);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 22_u8);
    assert_eq!(before["P2"]["leader"]["life"], 18_u8);
    assert!(
        before["objects"]["lloyd"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
    let step = game.play_internal_card("feast1", actor()).unwrap();
    answer(&mut game, step, "BP08-T01");
    assert_eq!(drain(&mut game, ""), 1);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 16_u8);
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("lloyd"))
    );
}
