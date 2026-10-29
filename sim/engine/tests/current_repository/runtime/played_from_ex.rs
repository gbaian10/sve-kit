#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real-card entry scenarios."
)]
use super::*;

fn position(card: &str, from: &str, seat: &str) -> Value {
    let mut initial = draw_position();
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["field"] = json!([]);
    initial["players"][seat]["zones"][from] = json!([{"id":"source","card":card,"state":{"entered_from":if from == "ex" { "hand" } else { "ex" },"entered_by":"play","entered_by_play":true}}]);
    initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn enter(initial: &Value, from: &str, played: bool, seat: &str) -> Game {
    if played {
        let mut game = game(initial);
        assert!(
            matches!(
                game.play_internal_card(
                    "source",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ),
            "a real card play enters for its printed cost"
        );
        game
    } else {
        let origin = if from == "cemetery" {
            ZoneKind::Cemetery
        } else {
            ZoneKind::Ex
        };
        let mut game = with_producer(
            initial,
            Action::Move {
                objects: subjects_in(origin),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Field,
                        },
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            },
        );
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        game
    }
}

fn trigger(game: &mut Game, expected: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed entry or evolution trigger");
    };
    assert_eq!(
        options.len(),
        1_usize,
        "one real printed ability is pending"
    );
    assert_eq!(
        options[0].card, expected,
        "the actual printing supplies its ability"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn select(game: &mut Game, play: CardPlay, number: &str) {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target is required before checking the origin");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal public target");
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(number))
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap_err();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
}

#[test]
fn played_from_ex_bp17_067_declares_target_for_hand_and_ex_but_only_ex_play_deals_damage() {
    for seat in ["P1", "P2"] {
        for (from, played) in [
            ("hand", true),
            ("ex", true),
            ("ex", false),
            ("cemetery", false),
        ] {
            let initial = position("BP17-067", from, seat);
            let mut game = enter(&initial, from, played, seat);
            let play = trigger(&mut game, "BP17-067");
            select(&mut game, play, "BP04-117");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if played && from == "ex" { 2_u8 } else { 0_u8 },
                "the sealed play origin controls the printed damage"
            );
            assert_eq!(
                state[seat]["pp"]["current"],
                if played { 8_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn played_from_ex_bp17_067_cannot_play_fanfare_without_a_formal_target() {
    for from in ["hand", "ex"] {
        let mut initial = position("BP17-067", from, "P1");
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = enter(&initial, from, true, "P1");
        assert!(
            matches!(trigger(&mut game, "BP17-067"), CardPlay::Cancelled),
            "even the false branch must respect the declared target"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
    }
}

#[test]
fn played_from_ex_bp17_068_deals_two_after_a_real_evolution_from_either_origin() {
    for from in ["hand", "ex"] {
        let mut initial = position("BP17-067", from, "P1");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-068"}]);
        let mut game = enter(&initial, from, true, "P1");
        let play = trigger(&mut game, "BP17-067");
        select(&mut game, play, "BP04-117");
        game.resume_internal_card().unwrap();
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(
            step.outcome, "resolved",
            "the real base pays one PP to evolve"
        );
        let evolution = trigger(&mut game, "BP17-068");
        select(&mut game, evolution, "BP04-117");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if from == "ex" { 4_u8 } else { 2_u8 }
        );
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn played_from_ex_bp15_069_and_alias_upgrade_the_other_ocean_follower_only_after_ex_play() {
    for card in ["BP15-069", "PR-391"] {
        for (from, played) in [("hand", true), ("ex", true), ("ex", false)] {
            let mut initial = position(card, from, "P1");
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP17-067"}]);
            let mut game = enter(&initial, from, played, "P1");
            let play = trigger(&mut game, card);
            select(&mut game, play, "BP17-067");
            let state = game.projection(View::Referee).unwrap();
            let upgraded = played && from == "ex";
            assert_eq!(
                state["objects"]["ally"]["power"],
                if upgraded { 4_u8 } else { 3_u8 }
            );
            assert_eq!(
                state["objects"]["ally"]["hp"],
                if upgraded { 4_u8 } else { 3_u8 }
            );
            for keyword in ["突進", "指定攻撃"] {
                assert_eq!(
                    state["objects"]["ally"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(keyword)),
                    upgraded,
                    "only the EX play grants each printed keyword"
                );
            }
            assert_eq!(
                state["objects"]["source"]["power"], 2_u8,
                "the source itself is never the other follower"
            );
        }
    }
}
