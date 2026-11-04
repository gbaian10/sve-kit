#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected public decisions invalidate these printed mode scenarios."
)]
use super::union_burst_batch1::{pending, ub};
use super::*;

fn aliases(representative: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[representative].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(printing, _)| printing.clone())
        .collect()
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-040","state":{"hp":50_u8,"max_hp":50_u8}}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..8)
                .map(|n| json!({"id":format!("{seat}-draw{n}"),"card":"CP04-071"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}
fn princess(initial: &mut Value) {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(6_u8);
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "final-light",
    )
    .unwrap()
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut play: CardPlay, mode: u64) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(false),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Quick { .. } => Answer::Quick(None),
            InputDomain::Modes { modes, .. } => {
                let chosen = if mode == 0 {
                    *modes.first().unwrap()
                } else {
                    ModeId(mode)
                };
                assert!(
                    modes.contains(&chosen),
                    "the printed mode must remain available to this instance"
                );
                Answer::Modes(vec![chosen])
            }
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let count = usize::try_from(
                    domain
                        .cardinality
                        .maximum
                        .unwrap_or(domain.cardinality.minimum)
                        .0,
                )
                .unwrap()
                .min(domain.members.len());
                let handles = domain
                    .members
                    .iter()
                    .take(count)
                    .map(|member| member.handle)
                    .collect();
                if matches!(&question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            _ => panic!("unexpected final-light input"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn terminal(play: &CardPlay) {
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_)
                | CardPlay::Entered(_)
                | CardPlay::Cancelled
                | CardPlay::Continued(_)
        ),
        "every phase or optional occurrence must finish"
    );
}
fn settle(game: &mut Game, mode: u64) {
    loop {
        checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        terminal(&finish(game, play, mode));
    }
}
fn activated(game: &mut Game, source: &str, card: &str) -> CardPlay {
    let id = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    game.activate_internal_card(source, id, PlayerId { seat: 1 })
        .unwrap()
}
fn realm_mode(game: &mut Game, source: &str, card: &str, expected: &[u64], selected: u64) {
    let play = activated(game, source, card);
    let CardPlay::Awaiting(question) = &play else {
        panic!("printed mode choice");
    };
    let InputDomain::Modes { modes, .. } = &question.domain else {
        panic!("modes");
    };
    assert_eq!(
        modes,
        &expected.iter().copied().map(ModeId).collect::<Vec<_>>(),
        "used options are excluded separately for this source and turn"
    );
    resolved(finish(game, play, selected));
    checks(game);
}
fn magic(game: &mut Game, id: &str) {
    let play = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    resolved(finish(game, play, 0));
    settle(game, 0);
}
fn end(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(game, 0);
    while let Some(question) = game.internal_quick_choice().unwrap() {
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        terminal(&finish(game, play, 0));
        settle(game, 0);
    }
}

#[test]
fn cleansing_realm_cycles_each_mode_independently_then_keeps_its_rule_stand_block_and_resets_next_turn()
 {
    for card in aliases("BP11-092") {
        let mut initial = position(&card);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"other","card":card}),
                json!({"id":"ally","card":"BP01-040"}),
            ]);
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (1_u8..=3)
                .map(|n| json!({"id":format!("item{n}"),"card":"CP02-T02"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        let original = board.projection(View::Referee).unwrap()["objects"]["ally"]["power"]
            .as_u64()
            .unwrap();
        realm_mode(&mut board, "source", &card, &[1, 2, 3], 1);
        realm_mode(&mut board, "other", &card, &[1, 2, 3], 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            44_u8
        );
        magic(&mut board, "item1");
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            false
        );
        realm_mode(&mut board, "source", &card, &[2, 3], 2);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["ally"]["power"]
                .as_u64()
                .unwrap(),
            original + 1
        );
        magic(&mut board, "item2");
        realm_mode(&mut board, "source", &card, &[3], 3);
        let created = board.projection(View::Referee).unwrap();
        let tigers = created["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| created["objects"][id.as_str().unwrap()]["name"] == "ホーリータイガー")
            .collect::<Vec<_>>();
        assert_eq!(tigers.len(), 1);
        let tiger = &created["objects"][tigers[0].as_str().unwrap()];
        assert_eq!(tiger["power"], 4_u8);
        assert_eq!(tiger["hp"], 4_u8);
        assert!(
            tiger["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進"))
        );
        assert_eq!(created["objects"]["source"]["acted"], true);
        end(&mut board);
        end(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            true
        );
        magic(&mut board, "item3");
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            false
        );
        realm_mode(&mut board, "source", &card, &[1, 2, 3], 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            41_u8
        );
    }
}

fn ranpha_entry(card: &str, zone: &str) {
    let mut initial = position(card);
    princess(&mut initial);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    let mut board = game(&initial);
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real four-cost follower enters from the selected public play origin"
    );
    let play = pending(&mut board, card);
    let outcome = resolved(finish(&mut board, play, 0));
    ub(&outcome);
    settle(&mut board, 0);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["enemy"]["hp"], 45_u8,
        "the UB fanfare deals its printed five damage"
    );
    assert_eq!(
        after["P1"]["pp"]["current"],
        if zone == "hand" { 6_u8 } else { 8_u8 },
        "only a non-Hand entry recovers two PP"
    );
    assert_eq!(
        after["P2"]["leader"]["life"], 20_u8,
        "the follower does not observe its own UB"
    );
}
fn ranpha_modes(card: &str) {
    let mut initial = position(card);
    princess(&mut initial);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"other","card":"CP04-SP10a"}),
            json!({"id":"ninon1","card":"CP04-035"}),
            json!({"id":"ninon2","card":"CP04-035"}),
        ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"healer","card":"CP04-001"}]);
    let mut board = game(&initial);
    let first = activated(&mut board, "ninon1", "CP04-035");
    ub(&resolved(finish(&mut board, first, 0)));
    settle(&mut board, 1);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        16_u8,
        "each different observer instance may use its own first mode"
    );
    let second = activated(&mut board, "ninon2", "CP04-035");
    ub(&resolved(finish(&mut board, second, 0)));
    settle(&mut board, 2);
    let twice = board.projection(View::Referee).unwrap();
    assert_eq!(
        twice["P1"]["leader"]["life"], 10_u8,
        "both observers choose their unused healing mode"
    );
    assert_eq!(
        twice["P1"]["pp"]["current"], 2_u8,
        "two Ninon abilities paid four PP each"
    );
    assert!(
        matches!(
            board
                .play_internal_card("healer", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "Kokkoro enters while her UB is effective at ten life"
    );
    let third = pending(&mut board, "CP04-001");
    ub(&resolved(finish(&mut board, third, 0)));
    settle(&mut board, 0);
    let exhausted = board.projection(View::Referee).unwrap();
    assert_eq!(
        exhausted["P1"]["leader"]["life"], 11_u8,
        "only Kokkoro heals after both observer modes were used"
    );
    assert_eq!(
        exhausted["P2"]["leader"]["life"], 16_u8,
        "exhausted observers cannot repeat damage mode"
    );
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "no unplayable mode occurrence remains waiting"
    );
}
#[test]
fn ranpha_and_misora_all_printings_check_entry_origin_and_exclude_used_modes_per_observer() {
    for card in aliases("CP04-SP10a") {
        ranpha_entry(&card, "hand");
        ranpha_entry(&card, "ex");
        ranpha_modes(&card);
    }
}
