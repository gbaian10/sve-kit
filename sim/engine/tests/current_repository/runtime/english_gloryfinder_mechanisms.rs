use super::admission_bp_cp_fixture::{actor, evolve, game, position, settle};
use super::english_cards::{settle_all, state, take_all};
use super::*;
use sve_engine::game::internal::{DecisionView, TargetRoleAssignment};

fn handles(question: &DecisionView, cards: &[&str]) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target question");
    };
    let mut used = Vec::new();
    for card in cards {
        let member = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some(*card) && !used.contains(&member.handle))
            .unwrap();
        used.push(member.handle);
    }
    used
}

/// Answers the current target question with the named printed cards, in order.
fn pick(board: &mut Game, step: &CardPlay, cards: &[&str]) -> CardPlay {
    let CardPlay::Awaiting(question) = step else {
        panic!("a target question is expected");
    };
    let picked = handles(question, cards);
    board
        .answer_internal_card(question.actor, question.token, Answer::Targets(picked))
        .unwrap()
}

fn on_field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn deck(cards: &[(&str, &str)]) -> Value {
    json!(
        cards
            .iter()
            .map(|(id, card)| json!({"id":id,"card":card}))
            .collect::<Vec<_>>()
    )
}

/// The printed ability on `line`; a card's Evolve keyword is also an activated ability.
fn ability_on_line(card: &str, line: u32) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id
}

/// Resolves the pending triggered ability and returns its first question.
fn triggered(board: &mut Game) -> CardPlay {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("a triggered ability is pending");
    };
    board
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
}

fn end_phase(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(board);
}

#[test]
fn arthur_reveals_ten_for_a_swordcraft_leader_and_summons_cheap_followers() {
    let top = [
        ("goblin", "BP01-171"),
        ("sage", "BP01-037"),
        ("mage", "BP01-081"),
        ("rabbit", "BP01-121"),
        ("fate", "BP01-161"),
        ("f5", "BP01-161"),
        ("f6", "BP01-161"),
        ("f7", "BP01-161"),
        ("f8", "BP01-161"),
        ("f9", "BP01-161"),
        ("rest", "BP01-171"),
    ];
    for (class, revealed) in [("ロイヤル", true), ("エルフ", false)] {
        let mut initial = position("GFB01a-005EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["deck"] = deck(&top);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"goblin-form","card":"BP01-172"}]);
        let mut board = game(&initial);
        let step = board.play_internal_card("source", actor()).unwrap();
        take_all(&mut board, step);
        settle_all(&mut board);
        let after = state(&board);
        if revealed {
            assert_eq!(after["P1"]["field"], json!(["source", "goblin", "mage"]));
            // "Whenever a follower is put onto your field, evolve it" uses the matching form.
            assert_eq!(after["objects"]["goblin"]["evolved_with"], "goblin-form");
            let rest = after["P1"]["deck"].as_array().unwrap();
            assert_eq!(rest[0], json!("rest"), "the eleventh card stays on top");
            assert_eq!(rest.len(), 9);
        } else {
            assert_eq!(after["P1"]["field"], json!(["source"]), "{class}");
            assert_eq!(after["P1"]["deck"], deck(&top)
                .as_array()
                .unwrap()
                .iter()
                .map(|card| card["id"].clone())
                .collect::<Value>());
        }
    }
}

fn icarus_hand(initial: &mut Value) {
    initial["players"]["P1"]["zones"]["deck"] =
        deck(&[("d0", "BP01-171"), ("d1", "BP01-171"), ("d2", "BP01-171")]);
}

#[test]
fn icarus_puts_two_cards_on_the_bottom_in_chosen_order_and_draws_two() {
    let mut initial = position("GFB01b-018EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"GFB01b-018EN"},
        {"id":"rabbit","card":"BP01-121"},
        {"id":"sage","card":"BP01-037"},
        {"id":"cerberus","card":"BP01-101"}
    ]);
    icarus_hand(&mut initial);
    let mut board = game(&initial);
    let played = board.play_internal_card("source", actor()).unwrap();
    take_all(&mut board, played);
    let mut step = triggered(&mut board);
    step = pick(&mut board, &step, &["BP01-121"]);
    step = pick(&mut board, &step, &["BP01-037"]);
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["hand"], json!(["cerberus", "d0", "d1"]));
    assert_eq!(after["P1"]["deck"], json!(["d2", "rabbit", "sage"]));
}

#[test]
fn evolved_icarus_may_put_back_fewer_cards_and_draws_that_many() {
    let mut initial = on_field("GFB01b-018EN");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"rabbit","card":"BP01-121"},{"id":"sage","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"GFB01b-019EN"}]);
    icarus_hand(&mut initial);
    let mut board = game(&initial);
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    let mut step = triggered(&mut board);
    step = pick(&mut board, &step, &["BP01-037"]);
    step = pick(&mut board, &step, &[]);
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["hand"], json!(["rabbit", "d0"]));
    assert_eq!(after["P1"]["deck"], json!(["d1", "d2", "sage"]));
}

#[test]
fn dragonrend_quake_trades_damage_between_two_distinct_followers() {
    let mut initial = position("GFB01b-023EN");
    initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"sage","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"goblin","card":"BP01-171"}]);
    let mut board = game(&initial);
    let played = board.play_internal_card("source", actor()).unwrap();
    let CardPlay::Awaiting(question) = &played else {
        panic!("the spell selects two followers");
    };
    let [sage, goblin] = handles(question, &["BP01-037", "BP01-171"])[..] else {
        panic!("both followers are candidates");
    };
    let roles = |first: u64, second: u64| Answer::TargetsWithRoles {
        targets: vec![first, second],
        roles: vec![
            TargetRoleAssignment {
                role: Nat(0),
                targets: vec![first],
            },
            TargetRoleAssignment {
                role: Nat(1),
                targets: vec![second],
            },
        ],
    };
    board
        .answer_internal_card(question.actor, question.token, roles(sage, sage))
        .unwrap_err();
    let step = board
        .answer_internal_card(question.actor, question.token, roles(goblin, sage))
        .unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P2"]["cemetery"], json!(["goblin"]));
    assert_eq!(after["objects"]["sage"]["damage"], 2_u8);
    assert_eq!(after["P1"]["pp"]["current"], 0_u8, "a Dragoncraft leader pays 1");
}

#[test]
fn purson_discards_cards_of_different_classes_and_draws_that_many() {
    let mut initial = position("GFB01c-003EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"GFB01c-003EN"},
        {"id":"rabbit","card":"BP01-121"},
        {"id":"cerberus","card":"BP01-101"},
        {"id":"sage","card":"BP01-037"},
        {"id":"goblin","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        deck(&[("d0", "BP01-171"), ("d1", "BP01-171"), ("d2", "BP01-171"), ("d3", "BP01-171")]);
    let mut board = game(&initial);
    let step = board
        .activate_internal_card("source", ability_on_line("GFB01c-003EN", 3), actor())
        .unwrap();
    let CardPlay::Awaiting(question) = &step else {
        panic!("the discard selects cards of different classes");
    };
    let same_class = handles(question, &["BP01-121", "BP01-101"]);
    board
        .answer_internal_card(question.actor, question.token, Answer::Targets(same_class))
        .unwrap_err();
    let step = pick(&mut board, &step, &["BP01-121", "BP01-037", "BP01-171"]);
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["hand"], json!(["cerberus", "d0", "d1", "d2"]));
    assert_eq!(
        after["P1"]["cemetery"],
        json!(["source", "rabbit", "sage", "goblin"])
    );
    assert_eq!(after["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn evolved_purson_summons_the_first_two_revealed_followers() {
    let mut initial = on_field("GFB01c-003EN");
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"GFB01c-004EN"}]);
    initial["players"]["P1"]["zones"]["deck"] = deck(&[
        ("fate", "BP01-161"),
        ("goblin", "BP01-171"),
        ("charm", "BP01-041"),
        ("rabbit", "BP01-121"),
        ("rest", "BP01-171"),
    ]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["field"], json!(["source", "goblin", "rabbit"]));
    let rest = after["P1"]["deck"].as_array().unwrap();
    assert_eq!(rest[0], json!("rest"));
    assert_eq!(rest.len(), 3);
}

#[test]
fn evolved_laina_punishes_an_opponent_holding_the_highest_attack() {
    for (mine, punished) in [(None, true), (Some("BP01-037"), false)] {
        let mut initial = position("GFB01d-001EN");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![json!({
            "id":"source","card":"GFB01d-001EN","state":{"evolved":true,"evolved_with":"form"}
        })];
        if let Some(card) = mine {
            field.push(json!({"id":"mine","card":card}));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P1"]["zones"]["evolution"] =
            json!([{"id":"form","card":"GFB01d-002EN"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"goblin","card":"BP01-171"}]);
        let mut board = game(&initial);
        end_phase(&mut board);
        assert_eq!(
            state(&board)["P2"]["leader"]["life"],
            if punished { 16_u8 } else { 20_u8 },
            "{mine:?}"
        );
    }
}

#[test]
fn golden_city_lion_lets_each_player_recover_an_amulet() {
    let mut initial = position("GFB01d-005EN");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"mine","card":"BP01-041"},{"id":"dead","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"theirs","card":"BP01-041"}]);
    let mut board = game(&initial);
    let step = board.play_internal_card("source", actor()).unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["hand"], json!(["mine"]));
    assert_eq!(after["P2"]["hand"], json!(["theirs"]));
    assert_eq!(after["P1"]["cemetery"], json!(["dead"]));
}

#[test]
fn golden_city_lion_summons_an_amulet_from_hand() {
    let mut initial = on_field("GFB01d-005EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"charm","card":"BP01-041"}]);
    let mut board = game(&initial);
    let step = board
        .activate_internal_card("source", activated("GFB01d-005EN"), actor())
        .unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["field"], json!(["source", "charm"]));
    assert_eq!(after["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn themis_purge_destroys_the_opponents_followers_and_heals_per_follower() {
    let mut initial = position("GFB01d-023EN");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"mine","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"g1","card":"BP01-171"},{"id":"g2","card":"BP01-121"}]);
    let mut board = game(&initial);
    let step = board.play_internal_card("source", actor()).unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!([]));
    assert_eq!(after["P1"]["field"], json!(["mine"]));
    // The destroyed Rabbit Necromancer's Last Words also hits both leaders for 2.
    assert_eq!(after["P1"]["leader"]["life"], 10_u8);
}

/// Answers every target question with as many candidates as allowed, putting `preferred`
/// first, until the scenario settles.
fn take_preferring(board: &mut Game, mut step: CardPlay, preferred: &str) {
    loop {
        while let CardPlay::Awaiting(question) = &step {
            let InputDomain::Targets(domain) = &question.domain else {
                take_all(board, step);
                break;
            };
            let allowed = domain
                .cardinality
                .maximum
                .map_or(domain.members.len(), |maximum| {
                    usize::try_from(maximum.0).unwrap()
                });
            let mut members = domain.members.iter().collect::<Vec<_>>();
            members.sort_by_key(|member| member.card.as_deref() != Some(preferred));
            let chosen = members
                .into_iter()
                .take(allowed)
                .map(|member| member.handle)
                .collect();
            step = board
                .answer_internal_card(question.actor, question.token, Answer::Targets(chosen))
                .unwrap();
        }
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(pending) = board.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("a pending ability is expected");
        };
        step = board
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
            .unwrap();
    }
}

#[test]
fn themis_purge_discarded_by_an_enemy_ability_destroys_an_enemy_follower_and_draws() {
    let mut initial = position("GFB01c-001EN");
    initial["turn"]["active"] = json!("P2");
    let mut hand = vec![json!({"id":"themis","card":"GFB01d-023EN"})];
    hand.extend((0_u8..6).map(|index| json!({"id":format!("held-{index}"),"card":"BP01-171"})));
    initial["players"]["P1"]["zones"]["hand"] = json!(hand);
    initial["players"]["P1"]["zones"]["deck"] = deck(&[("d0", "BP01-171"), ("d1", "BP01-171")]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"milnard","card":"GFB01c-001EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    let step = board
        .play_internal_card("milnard", PlayerId { seat: 2 })
        .unwrap();
    take_preferring(&mut board, step, "BP01-037");
    let after = state(&board);
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("themis")),
        "{after}"
    );
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P2"]["field"], json!(["milnard"]));
    assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 5, "4 kept and 1 drawn");
}

fn evolved_followers(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            json!({"id":format!("used-{index}"),"card":"BP01-172","face_up":true})
        })
        .collect()
}

#[test]
fn olivia_evolves_herself_with_ten_faceup_evolved_followers() {
    for (used, evolved) in [(10_usize, true), (9, false)] {
        let mut initial = position("GFE01-016EN");
        initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":10_u8});
        let mut evolve_deck = evolved_followers(used);
        evolve_deck.push(json!({"id":"form","card":"GFE01-017EN"}));
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!(evolve_deck);
        let mut board = game(&initial);
        let step = board.play_internal_card("source", actor()).unwrap();
        take_all(&mut board, step);
        settle_all(&mut board);
        let after = state(&board);
        if evolved {
            assert_eq!(after["objects"]["source"]["evolved_with"], "form");
            assert_eq!(after["P1"]["pp"]["current"], 5_u8, "On Evolve recovers 5");
        } else {
            assert!(after["objects"]["source"]["evolved_with"].is_null(), "{used}");
            assert_eq!(after["P1"]["pp"]["current"], 0_u8);
        }
    }
}

#[test]
fn evolved_olivia_evolves_the_first_follower_with_evolve_each_turn() {
    let mut initial = position("GFE01-016EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"rabbit","card":"BP01-121"},
        {"id":"aramis","card":"GFE01-003EN"},
        {"id":"porthos","card":"GFE01-003EN"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"GFE01-016EN","state":{"evolved":true,"evolved_with":"form"}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"GFE01-017EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"aramis-form","card":"GFE01-004EN"},
        {"id":"porthos-form","card":"GFE01-004EN"}
    ]);
    let mut board = game(&initial);
    for id in ["rabbit", "aramis", "porthos"] {
        let step = board.play_internal_card(id, actor()).unwrap();
        take_all(&mut board, step);
        settle_all(&mut board);
    }
    let after = state(&board);
    assert!(after["objects"]["rabbit"]["evolved_with"].is_null());
    assert_eq!(after["objects"]["aramis"]["evolved_with"], "aramis-form");
    assert!(
        after["objects"]["porthos"]["evolved_with"].is_null(),
        "once per turn"
    );
}
