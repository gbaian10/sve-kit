#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these card scenarios."
)]
use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(17_u8);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-drawn"),"card":"BP01-037"},
            {"id":format!("{seat}-next"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected target choice");
            };
            assert_eq!(
                domain.members.len(),
                1,
                "exactly the intended follower is eligible"
            );
            Answer::Targets(vec![domain.members[0].handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game, printing: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected one card trigger")
    };
    assert_eq!(options.len(), 1, "only the migrated source triggers");
    assert_eq!(
        options[0].card, printing,
        "the option preserves the effective printing"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Resolved(_)),
        "the selected printed trigger resolves"
    );
}

fn play(game: &mut Game, source: &str) -> CardPlay {
    let step = game
        .play_internal_card(source, PlayerId { seat: 1 })
        .unwrap();
    finish(game, step)
}

fn repair(game: &mut Game) {
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    let tokens = before["P1"]["ex"].as_array().unwrap();
    assert_eq!(tokens.len(), 1, "one Repair Mode was created");
    let token = tokens[0].as_str().unwrap().to_owned();
    assert!(
        ["BP07-T02", "BP12-T11", "BP17-T18", "PR-194"]
            .contains(&before["objects"][&token]["card"].as_str().unwrap()),
        "the generated spell belongs to the Repair Mode family"
    );
    let life = before["P1"]["leader"]["life"].as_u64().unwrap();
    let pp = before["P1"]["pp"]["current"].as_u64().unwrap();
    assert!(
        matches!(play(game, &token), CardPlay::Resolved(_)),
        "the generated spell plays from EX"
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["leader"]["life"],
        life.checked_add(1).unwrap(),
        "Repair Mode heals one"
    );
    assert_eq!(
        after["P1"]["pp"]["current"],
        pp.checked_sub(1).unwrap(),
        "Repair Mode costs one PP"
    );
    assert_eq!(after["P1"]["ex"], json!([]), "the EX spell leaves its zone");
    assert!(
        after["objects"].get(&token).is_none(),
        "the token ceases to exist"
    );
}

fn destroy_source(game: &mut Game) {
    assert!(
        matches!(play(game, "offering"), CardPlay::Resolved(_)),
        "Dark Offering destroys the real source"
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn coco_alias_buffs_an_own_follower_without_expiry() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"coco","card":"PR-074"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(matches!(play(&mut game, "coco"), CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["friend"]["power"], 7_u8);
    assert_eq!(state["objects"]["friend"]["hp"], 5_u8);
    assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    assert!(state["objects"].get("coco").is_none());
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    let next_turn = game.projection(View::Referee).unwrap();
    assert_eq!(next_turn["turn"]["active"], "P2");
    assert_eq!(next_turn["objects"]["friend"]["power"], 7_u8);
}

#[test]
fn tetra_alias_fanfare_creates_a_playable_repair_mode() {
    let mut initial = position();
    initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP07-SL07"}]);
    let mut game = game(&initial);
    assert!(matches!(play(&mut game, "source"), CardPlay::Entered(_)));
    trigger(&mut game, "BP07-SL07");
    repair(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn iron_sister_alias_fanfare_and_lastword_each_create_repair_mode() {
    let mut initial = position();
    initial["players"]["P1"]["pp"]["current"] = json!(5_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"PR-176"},{"id":"offering","card":"BP01-140"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(play(&mut game, "source"), CardPlay::Entered(_)));
    trigger(&mut game, "PR-176");
    repair(&mut game);
    game.resume_internal_card().unwrap();
    destroy_source(&mut game);
    trigger(&mut game, "PR-176");
    repair(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}

#[test]
fn mechagoblin_alias_lastword_creates_a_playable_repair_mode() {
    let mut initial = position();
    initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP07-P31"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"offering","card":"BP01-140"}]);
    let mut game = game(&initial);
    destroy_source(&mut game);
    trigger(&mut game, "BP07-P31");
    repair(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}

#[test]
fn turbulence_alias_banishes_an_enemy_and_ceases_to_exist() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"EBD04-T01"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-006"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(matches!(play(&mut game, "source"), CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    assert_eq!(state["P2"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    assert_eq!(state["P1"]["field"], json!(["friend"]));
    assert!(state["objects"].get("source").is_none());
}
