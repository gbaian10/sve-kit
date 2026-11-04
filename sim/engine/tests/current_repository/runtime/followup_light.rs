#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these printed light scenarios."
)]
use super::union_burst_batch1::{pending, ub};
use super::*;
use sve_engine::game::internal::SearchAnswer;

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
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-040"},{"id":"spare","card":"BP01-040"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-040","state":{"hp":50_u8}}]);
    initial
}
fn princess(initial: &mut Value) {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "followup-light",
    )
    .unwrap()
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(false),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
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
            InputDomain::Search(domain) => {
                let count = usize::try_from(
                    domain
                        .cardinality
                        .maximum
                        .unwrap_or(domain.cardinality.minimum)
                        .0,
                )
                .unwrap()
                .min(domain.cards.len());
                let picks = domain
                    .cards
                    .iter()
                    .take(count)
                    .map(|member| member.handle)
                    .collect::<Vec<_>>();
                Answer::Search(match &domain.groups {
                    None => SearchAnswer::Ungrouped(picks),
                    Some(groups) => {
                        assert_eq!(groups.len(), 1, "one printed distinct-name search group");
                        assert!(
                            picks
                                .iter()
                                .all(|handle| groups[0].handles.contains(handle)),
                            "every chosen handle belongs to the printed search group"
                        );
                        SearchAnswer::Grouped(vec![picks])
                    }
                })
            }
            _ => panic!("unexpected followup-light decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn trigger(game: &mut Game, card: &str) -> Activation {
    let play = pending(game, card);
    let outcome = resolved(finish(game, play));
    checks(game);
    outcome
}
fn settle(game: &mut Game) {
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
        resolved(finish(game, play));
    }
}
fn activation(card: &str) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| {
            entry.node.kind == AbilityKind::Activated
                && !matches!(
                    entry.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Evolution,
                        ..
                    }
                )
        })
        .unwrap()
        .node
        .id
}
fn activate(game: &mut Game, source: &str, card: &str) {
    let play = game
        .activate_internal_card(source, activation(card), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(game, play));
    checks(game);
}
fn forbidden(game: &mut Game, card: &str) {
    let error = game
        .activate_internal_card("source", activation(card), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(message) if message == "ability play is forbidden"),
        "the permission gate, rather than missing materials or targets, must reject activation"
    );
}
fn evolve(game: &mut Game, base: &str) {
    let definition = selection_with_rules().current[base]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| {
            matches!(
                entry.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Evolution,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let step = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":definition.0,"pay":{"pp":2_u8}})).unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the real base may still pay its printed evolution cost"
    );
}
fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    resolved(finish(game, step));
    settle(game);
}

#[test]
fn yuzu_base_printings_pay_lesson_bounce_another_follower_and_carry_the_lock_through_real_evolution()
 {
    for (base, form) in aliases("CP02-004").into_iter().zip(aliases("CP02-005")) {
        let mut initial = position(&base);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other","card":"CP02-004"}));
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"item1","card":"CP02-T02"},{"id":"item2","card":"CP02-T02"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"CP02-042"}]);
        let mut board = game(&initial);
        activate(&mut board, "source", &base);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["hand"], json!(["other"]));
        assert_eq!(after["P1"]["ex_count"], 1_u8);
        assert_eq!(after["P1"]["pp"]["current"], 10_u8);
        assert!(
            after["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
        forbidden(&mut board, &base);
        evolve(&mut board, &base);
        trigger(&mut board, &form);
        let evolved = board.projection(View::Referee).unwrap();
        assert_eq!(evolved["P1"]["pp"]["current"], 8_u8);
        assert_eq!(evolved["P2"]["hand"], json!(["enemy"]));
        assert_eq!(evolved["P2"]["field"], json!([]));
        forbidden(&mut board, &form);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["ex_count"],
            1_u8
        );
    }
}

#[test]
fn yuzu_evolution_printings_bounce_a_small_enemy_then_pay_lesson_and_remove_the_returned_tree_token()
 {
    for form in aliases("CP02-005") {
        let mut initial = position("CP02-004");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"tree","card":"BP07-T03"}));
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"item1","card":"CP02-T02"},{"id":"item2","card":"CP02-T02"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"CP02-042"}]);
        let mut board = game(&initial);
        evolve(&mut board, "CP02-004");
        trigger(&mut board, &form);
        activate(&mut board, "source", &form);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["field"], json!(["source"]));
        assert_eq!(after["P1"]["hand"], json!([]));
        assert!(after["objects"]["tree"].is_null());
        assert_eq!(after["P1"]["ex_count"], 1_u8);
        assert_eq!(after["P1"]["pp"]["current"], 8_u8);
        assert_eq!(after["P2"]["hand"], json!(["enemy"]));
        assert!(
            after["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
        forbidden(&mut board, &form);
    }
}

fn trio_search(card: &str) {
    let mut initial = position(card);
    princess(&mut initial);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":card},{"id":"proof","card":"CP04-071"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"a-chel","card":"CP04-047"},{"id":"b-dream","card":"CP04-044"},
        {"id":"z-duplicate","card":"CP04-044"}
    ]);
    let mut board = game(&initial);
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real trio enters before its UB fanfare is played"
    );
    let fanfare = trigger(&mut board, card);
    ub(&fanfare);
    settle(&mut board);
    let searched = board.projection(View::Referee).unwrap();
    assert_eq!(
        searched["P1"]["ex"],
        json!(["a-chel", "b-dream"]),
        "the search obtains two different printed names"
    );
    assert_eq!(
        searched["P1"]["pp"]["current"], 5_u8,
        "the normal follower paid its printed five PP"
    );
    play(&mut board, "proof");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8,
        "its own UB must not install a next-Spell discount"
    );
    play(&mut board, "b-dream");
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["pp"]["current"], 3_u8,
        "the searched Spell has its own two-point discount"
    );
    assert_eq!(
        after["P1"]["ex"],
        json!(["a-chel"]),
        "the played searched Spell leaves the other searched card in EX"
    );
}
fn trio_stack(card: &str) {
    let mut initial = position(card);
    princess(&mut initial);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"ninon1","card":"CP04-035"}),
            json!({"id":"ninon2","card":"CP04-035"}),
        ]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"spell1","card":"CP04-071"},{"id":"spell2","card":"CP04-071"}]);
    let mut board = game(&initial);
    for source in ["ninon1", "ninon2"] {
        activate(&mut board, source, "CP04-035");
        settle(&mut board);
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8,
        "each different Ninon pays four PP for her own UB"
    );
    play(&mut board, "spell1");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8,
        "both installed discounts apply to the first Spell"
    );
    play(&mut board, "spell2");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8,
        "both stacked next-only discounts were consumed by the first Spell"
    );
}
#[test]
fn the_friendship_trio_all_printings_search_and_discount_then_stack_other_ub_discounts_once() {
    for card in aliases("CP04-SP06a") {
        trio_search(&card);
        trio_stack(&card);
    }
}
