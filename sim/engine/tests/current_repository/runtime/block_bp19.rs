use super::event_cemetery::finish_checks;
use super::event_evolved::{evolved, from_position, position};
use super::*;

fn scenario(side: &str, played: usize) -> Game {
    let mut initial = position("BP19-005", side);
    initial["players"][side]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|card| card["id"] == "subject")
        .unwrap()["card"] = json!("BP19-005");
    initial["players"][side]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-006"}]);
    initial["players"][side]["zones"]["hand"] = json!(
        (0..played)
            .map(|i| json!({"id":format!("played-{i}"),"card":"BP02-118"}))
            .collect::<Vec<_>>()
    );
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_usize..3)
                .map(|i| json!({"id":format!("{seat}-draw-{i}"),"card":"evolve-enemy"}))
                .collect::<Vec<_>>()
        );
    }
    if side == "P2" {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"left-target","card":"evolve-enemy"}));
    }
    let mut game = from_position(&initial);
    let actor = PlayerId {
        seat: if side == "P1" { 1 } else { 2 },
    };
    for i in 0..played {
        assert!(
            matches!(
                game.play_internal_card(&format!("played-{i}"), actor)
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "each genuine two-cost neutral follower is normally played"
        );
        finish_checks(&mut game);
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the genuine Secteur has no Fanfare to replace or skip"
        );
    }
    game
}

fn evolve_subject(game: &mut Game, side: &str) {
    let pp = game.projection(View::Referee).unwrap()[side]["pp"]["current"]
        .as_u64()
        .unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"subject","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the genuine base evolves through its printed one-PP ability"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()[side]["pp"]["current"],
        pp.checked_sub(1).unwrap(),
        "the native evolution actually pays one PP"
    );
    finish_checks(game);
}

#[expect(
    clippy::panic,
    reason = "Only the native evolution keyword declares a damage target."
)]
fn finish_evolution_body(game: &mut Game, play: CardPlay) -> bool {
    if let CardPlay::Awaiting(declaration) = play {
        let InputDomain::Targets(domain) = &declaration.domain else {
            panic!("native OnEvolve damage target")
        };
        let enemy = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("evolve-enemy"))
            .unwrap();
        resolved(
            game.answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::Targets(vec![enemy.handle]),
            )
            .unwrap(),
        );
        true
    } else {
        resolved(play);
        false
    }
}

#[expect(
    clippy::panic,
    reason = "Every admitted evolution occurrence must use the public pending domain."
)]
fn resolve_evolution_heads(
    game: &mut Game,
    side: &str,
    expected_base: usize,
    expected_form: usize,
) {
    let mut base = 0_usize;
    let mut form = 0_usize;
    let mut targets = 0_usize;
    while let Some(occurrence) = game.internal_pending_choice().unwrap() {
        assert_eq!(
            occurrence.actor.seat,
            if side == "P1" { 1 } else { 2 },
            "the opposing observer never joins the evolution queue"
        );
        let InputDomain::Pending(options) = &occurrence.domain else {
            panic!("native evolution pending domain")
        };
        let option = options.first().unwrap();
        if option.card == "BP19-005" {
            base = base.checked_add(1).unwrap();
        } else {
            assert_eq!(
                option.card, "BP19-006",
                "only the genuine base observer or new form owns a head"
            );
            form = form.checked_add(1).unwrap();
        }
        let play = game
            .answer_internal_card(
                occurrence.actor,
                occurrence.token,
                Answer::Pending(option.handle),
            )
            .unwrap();
        targets = targets
            .checked_add(usize::from(finish_evolution_body(game, play)))
            .unwrap();
        finish_checks(game);
    }
    assert_eq!(
        base, expected_base,
        "only the other surviving base copy observes this evolution"
    );
    assert_eq!(
        form, expected_form,
        "the new form contributes one generic auto and one keyword occurrence"
    );
    assert_eq!(
        targets, 1,
        "the native OnEvolve damage declares exactly one formal target"
    );
}

fn zone_size(state: &Value, side: &str, zone: &str) -> usize {
    state[side][zone].as_array().unwrap().len()
}

#[test]
fn bp19_005_and_006_real_plays_feed_generic_and_keyword_heads_without_old_form_duplicates() {
    for played in [2, 3] {
        let mut game = scenario("P1", played);
        let before = game.projection(View::Referee).unwrap();
        evolve_subject(&mut game, "P1");
        resolve_evolution_heads(&mut game, "P1", 1, 2);
        let after = game.projection(View::Referee).unwrap();
        let draws = if played == 3 { 2 } else { 0 };
        assert_eq!(
            zone_size(&after, "P1", "hand"),
            zone_size(&before, "P1", "hand") + draws,
            "both actual generic observers draw only after three real card plays"
        );
        assert_eq!(
            zone_size(&after, "P1", "deck") + draws,
            zone_size(&before, "P1", "deck"),
            "each generic head draws exactly once"
        );
        assert_eq!(
            after["objects"]["enemy"]["damage"], 2_u8,
            "the distinct native keyword still deals two damage below or above Combo three"
        );
    }
}

#[test]
fn bp19_005_does_not_observe_an_opposing_prison_evolution_while_the_new_form_keeps_its_own_heads() {
    let mut game = scenario("P2", 3);
    let before = game.projection(View::Referee).unwrap();
    evolve_subject(&mut game, "P2");
    resolve_evolution_heads(&mut game, "P2", 0, 2);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["deck"], before["P1"]["deck"],
        "the opposing base observer never draws"
    );
    assert_eq!(
        after["P1"]["hand"], before["P1"]["hand"],
        "the opponent's hand remains unchanged"
    );
    assert_eq!(
        zone_size(&after, "P2", "hand"),
        zone_size(&before, "P2", "hand") + 1,
        "the newly evolved native form still observes its own evolution"
    );
    assert_eq!(
        after["objects"]["left-target"]["damage"], 2_u8,
        "the form's native keyword targets a durable enemy instead of removing the observer"
    );
}

#[test]
fn bp19_both_observers_require_the_evolved_subject_to_have_the_prison_trait() {
    for card in ["BP19-005", "BP19-006"] {
        let mut game = evolved(card, "P1");
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the genuine Ward-only subject has no qualifying Prison trait"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            0_u8,
            "an unrelated evolution does not play the observer's keyword"
        );
    }
}
