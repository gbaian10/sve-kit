#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn finish(game: &mut Game, mut step: CardPlay, modes: &[ModeId]) {
    while let CardPlay::Awaiting(question) = step {
        let answer = if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(modes.to_vec())
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else {
            panic!("unexpected decision")
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game, modes: &[ModeId]) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step, modes);
}

fn trigger(game: &mut Game, modes: &[ModeId]) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending printed ability")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed trigger is pending"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step, modes);
    game.resume_internal_card().unwrap();
}

fn activate(game: &mut Game, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(game, step, &[]);
    game.resume_internal_card().unwrap();
}

fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp02_010_modes_buff_a_fairy_or_search_a_fairy_follower() {
    for mode in [1_u64, 2] {
        let mut initial = position("BP02-010");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"BP15-013"},{"id":"spell","card":"BP01-023"}]);
        let mut game = game(&initial);
        play(&mut game, &[]);
        trigger(&mut game, &[ModeId(mode)]);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["fairy"]["power"],
            if mode == 1 { 3_u8 } else { 1_u8 }
        );
        assert_eq!(
            s["objects"]["fairy"]["hp"],
            if mode == 1 { 3_u8 } else { 1_u8 }
        );
        assert_eq!(
            s["P1"]["hand"],
            if mode == 2 {
                json!(["found"])
            } else {
                json!([])
            },
            "the chosen mode alone searches"
        );
        assert!(
            s["P1"]["deck"]
                .as_array()
                .unwrap()
                .contains(&json!("spell")),
            "a Fairy spell is not a Fairy follower"
        );
    }
}

#[test]
fn bp02_032_normal_evolution_searches_up_to_two_soldier_followers() {
    let mut initial = position("BP02-031");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-031"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-032"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"a","card":"BP01-040"},{"id":"b","card":"BP01-040"},{"id":"other","card":"BP01-026"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves for one PP"
    );
    trigger(&mut game, &[]);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["a", "b"]));
    assert_eq!(s["P1"]["deck"], json!(["other"]));
    assert_eq!(s["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp02_034_fanfare_searches_only_a_princess_follower() {
    let mut initial = position("BP02-034");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"found","card":"BP01-026"},{"id":"other","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, &[]);
    trigger(&mut game, &[]);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["found"]));
    assert_eq!(s["P1"]["deck"], json!(["other"]));
}

#[test]
fn bp02_017_creates_rose_strike_in_ex_and_can_rush_on_entry() {
    let mut game = game(&position("BP02-017"));
    play(&mut game, &[]);
    trigger(&mut game, &[]);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&s, "ex", "薔薇の一撃"),
        1,
        "Fanfare creates exactly the printed spell in EX"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not permit entry-turn leader attacks"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Rush permits the entry-turn follower attack"
    );
}

#[test]
fn bp02_099_creates_falcon_then_pays_and_sacrifices_for_tiger() {
    let mut game = game(&position("BP02-099"));
    play(&mut game, &[]);
    trigger(&mut game, &[]);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&before, "field", "ホーリーファルコン"),
        1,
        "the Fanfare creates a Falcon"
    );
    let pp = before["P1"]["pp"]["current"].as_u64().unwrap();
    activate(&mut game, "BP02-099");
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["source"]));
    assert_eq!(s["P1"]["pp"]["current"], pp.checked_sub(2).unwrap());
    assert_eq!(
        named(&s, "field", "ホーリーファルコン"),
        1,
        "the existing Falcon remains"
    );
    assert_eq!(
        named(&s, "field", "ホーリータイガー"),
        1,
        "the paid activation creates a Tiger"
    );
}

#[test]
fn bp02_045_spell_chain_increases_distinct_modes_from_two_to_three() {
    for chain in [9_usize, 10] {
        let mut initial = position("BP02-045");
        initial["players"]["P1"]["zones"]["cemetery"]=json!((0..chain).map(|n|json!({"id":format!("old{n}"),"card":(["BP01-007","BP01-015","BP01-023","BP01-049","BP01-050","BP01-064","BP01-089","BP01-090","BP01-098","BP01-099"][n])})).collect::<Vec<_>>());
        let modes = if chain == 9 {
            vec![ModeId(2), ModeId(3)]
        } else {
            vec![ModeId(1), ModeId(2), ModeId(3)]
        };
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game.play_internal_card("source", actor()).unwrap()
        else {
            panic!("formal mode selection")
        };
        let InputDomain::Modes {
            count, cardinality, ..
        } = &question.domain
        else {
            panic!("mode bounds")
        };
        assert_eq!(
            *count,
            Nat(if chain == 9 { 2 } else { 3 }),
            "Spellchain ten increases the mode cap from two to three"
        );
        assert_eq!(cardinality.minimum, Nat(1));
        assert!(
            game.answer_internal_card(question.actor, question.token, Answer::Modes(vec![]))
                .is_err(),
            "at least one mode must be chosen (Q293)"
        );
        assert!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2), ModeId(2)])
            )
            .is_err(),
            "one mode cannot be chosen twice (Q294)"
        );
        if chain == 9 {
            assert!(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Modes(vec![ModeId(1), ModeId(2), ModeId(3)])
                )
                .is_err(),
                "nine cemetery spells cannot choose all three modes"
            );
        }
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Modes(modes))
            .unwrap();
        finish(&mut game, step, &[]);
        game.resume_internal_card().unwrap();
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["enemy"]["damage"],
            if chain == 9 { 0_u8 } else { 3_u8 }
        );
        assert_eq!(
            named(&s, "field", "防御型ゴーレム"),
            1,
            "one chosen mode creates one Golem"
        );
        assert_eq!(s["P1"]["hand"], json!(["drawn"]));
        assert_eq!(s["P1"]["pp"]["current"], 6_u8);
    }
}

fn castle(card: &str) {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"first","card":"BP01-171"},{"id":"picked","card":"BP02-076"},{"id":"third","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, &[]);
    trigger(&mut game, &[]);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["hand"],
        json!(["picked"]),
        "the Vampire amulet is a legal card to take"
    );
    assert_eq!(
        before["P1"]["deck"],
        json!(["tail", "third", "first"]),
        "only unchosen observed cards move to the bottom in chosen order"
    );
    activate(&mut game, card);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["cemetery"],
        json!(["source"]),
        "the amulet pays its sacrifice cost"
    );
    assert_eq!(
        s["P1"]["pp"]["current"], 8_u8,
        "playing the amulet and activating it each cost one PP"
    );
    assert_eq!(
        named(&s, "field", "フォレストバット"),
        1,
        "sacrificing the amulet creates the printed Bat"
    );
}

#[test]
fn bp02_076_looks_for_a_vampire_card_then_sacrifices_for_a_bat() {
    castle("BP02-076");
}

#[test]
fn bp02_077_collaboration_printing_looks_then_sacrifices_for_a_bat() {
    castle("BP02-077");
}
