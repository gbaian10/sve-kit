#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed lifecycle scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn definition(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.source.line == line)
        .unwrap()
        .node
        .id
}

fn finish(game: &mut Game, mut play: CardPlay, accept: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the printed recipient is eligible"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected lifecycle decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed effect completes"
    );
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

pub(super) fn trigger(game: &mut Game, number: &str, line: u64, accept: bool) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    let chosen = options
        .iter()
        .find(|entry| entry.card == number && entry.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(chosen.handle),
        )
        .unwrap();
    finish(game, play, accept);
    finish_checks(game);
}

pub(super) fn departed(number: &str, counter: &str, count: u8) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"killer","card":"BP03-004","state":{"counters":{"童話カウンター":1_u8}}}
    ]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"victim","card":number,"state":{"counters":{counter:count}}}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let play = game
        .activate_internal_card("killer", definition("BP03-004", 2), PLAYER)
        .unwrap();
    finish(&mut game, play, true);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["victim"]),
        "the real opposing Lila destroys the printed source before its trigger"
    );
    game
}

#[test]
fn snow_white_and_printing_use_the_departed_counter_and_return_without_that_counter() {
    for number in ["BP03-090", "BP03-SL17"] {
        for count in [0_u8, 1] {
            let mut game = departed(number, "童話カウンター", count);
            trigger(&mut game, number, 4, true);
            let view = game.projection(View::Referee).unwrap();
            if count == 0 {
                assert_eq!(view["P2"]["cemetery"], json!(["victim"]));
                assert_eq!(view["P2"]["ex"], json!([]));
            } else {
                assert_eq!(view["P2"]["cemetery"], json!([]));
                assert_eq!(view["P2"]["ex"], json!(["victim"]));
                assert_eq!(view["objects"]["victim"]["generation"], 2_u8);
            }
            assert_eq!(
                view["objects"]["victim"]["counters"]["童話カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                0
            );
        }
    }
}

#[test]
fn winged_prince_adds_only_to_the_successful_new_ex_generation_and_may_decline() {
    for full in [false, true] {
        for accept in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-102"}]);
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"recovered","card":"BP03-004"}]);
            initial["players"]["P1"]["zones"]["ex"] = if full {
                json!([
                    {"id":"ex-1","card":"BP03-004"}, {"id":"ex-2","card":"BP03-004"},
                    {"id":"ex-3","card":"BP03-004"}, {"id":"ex-4","card":"BP03-004"},
                    {"id":"ex-5","card":"BP03-004"}
                ])
            } else {
                json!([])
            };
            let mut game = game(&initial);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play, accept);
            finish_checks(&mut game);
            trigger(&mut game, "BP03-102", 2, accept);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 5_u8);
            if full {
                assert_eq!(view["P1"]["cemetery"], json!(["recovered"]));
                assert_eq!(view["objects"]["recovered"]["generation"], 0_u8);
            } else {
                assert_eq!(view["P1"]["ex"], json!(["recovered"]));
                assert_eq!(view["objects"]["recovered"]["generation"], 1_u8);
            }
            assert_eq!(
                view["objects"]["recovered"]["counters"]["童話カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                u64::from(accept && !full)
            );
        }
    }
}

#[test]
fn aluzard_and_printing_gain_two_dormancy_counters_only_on_the_new_ex_generation() {
    for number in ["BP13-071", "BP13-SL15"] {
        let mut game = departed(number, "休眠カウンター", 99);
        trigger(&mut game, number, 4, true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["ex"], json!(["victim"]));
        assert_eq!(view["objects"]["victim"]["generation"], 2_u8);
        assert_eq!(
            view["objects"]["victim"]["counters"]["休眠カウンター"],
            2_u8
        );
    }
}

fn dormancy_position(number: &str, count: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"source","card":number,"state":{"counters":{"休眠カウンター":count}}}
    ]);
    initial
}

#[test]
fn aluzard_cannot_play_from_ex_until_dormancy_is_zero_but_can_play_from_hand() {
    for number in ["BP13-071", "BP13-SL15"] {
        let mut blocked = game(&dormancy_position(number, 1));
        let before = format!("{blocked:?}");
        let rejected = blocked.play_internal_card("source", PLAYER).unwrap_err();
        assert!(
            matches!(rejected, EngineFailure::Unsupported(message)
            if message.contains("card play holder, permission or timing")),
            "the printed restriction rejects at the card permission boundary"
        );
        assert!(
            format!("{blocked:?}") == before,
            "a forbidden play preserves authority"
        );
        for zone in ["hand", "ex"] {
            let mut initial = dormancy_position(number, 0);
            initial["players"]["P1"]["zones"]["ex"] = json!([]);
            initial["players"]["P1"]["zones"][zone] = if zone == "ex" {
                json!([{"id":"source","card":number,"state":{"counters":{"休眠カウンター":0_u8}}}])
            } else {
                json!([{"id":"source","card":number}])
            };
            let mut allowed = game(&initial);
            let play = allowed.play_internal_card("source", PLAYER).unwrap();
            finish(&mut allowed, play, true);
            let view = allowed.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["field"], json!(["source"]));
            assert_eq!(view["P1"]["pp"]["current"], 8_u8);
        }
    }
}

#[test]
fn aluzard_main_trigger_pays_one_and_returns_only_after_the_last_counter_or_can_decline() {
    for number in ["BP13-071", "BP13-SL15"] {
        for (count, accept) in [(0_u8, true), (1, true), (2, true), (1, false)] {
            let mut initial = dormancy_position(number, count);
            initial["turn"]["active"] = json!("P2");
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"turn-draw","card":"BP01-037"},
                {"id":"remaining","card":"BP01-037"}
            ]);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
            let quick = game.internal_quick_choice().unwrap().unwrap();
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("own main trigger")
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, number);
            assert_eq!(options[0].line, 3_u64);
            let mut play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            if let CardPlay::Awaiting(declare) = play {
                assert_eq!(declare.domain, InputDomain::OptionalCost);
                play = game
                    .answer_internal_card(
                        declare.actor,
                        declare.token,
                        Answer::OptionalCost(accept),
                    )
                    .unwrap();
            }
            if count == 0 || !accept {
                assert!(
                    matches!(play, CardPlay::Cancelled),
                    "an unpaid trigger never enters for free"
                );
            } else {
                finish(&mut game, play, true);
            }
            finish_checks(&mut game);
            let view = game.projection(View::Referee).unwrap();
            let enters = count == 1 && accept;
            assert_eq!(
                view["P1"]["field"],
                if enters { json!(["source"]) } else { json!([]) }
            );
            assert_eq!(
                view["P1"]["ex"],
                if enters { json!([]) } else { json!(["source"]) }
            );
            let expected = if accept {
                count.saturating_sub(1)
            } else {
                count
            };
            assert_eq!(
                view["objects"]["source"]["counters"]["休眠カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                u64::from(expected)
            );
        }
    }
}
