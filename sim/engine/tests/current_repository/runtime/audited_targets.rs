use super::admission_fixture::finish;
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::player::observation::ProcessingStage;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn pending_play(game: &mut Game, card: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("{card}: expected a pending ability");
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn field_entry(game: &mut Game) {
    let entered = game.play_internal_card("source", PLAYER).unwrap();
    finish(game, entered);
    finish_checks(game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source"]),
        "the card enters before its Fanfare is played"
    );
}

fn declares_target(game: &Game, play: &CardPlay, card: &str, target_count: usize) {
    let CardPlay::Awaiting(question) = play else {
        panic!("{card}: expected a formal target question");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("{card}: expected targets");
    };
    assert_eq!(domain.cardinality.minimum, Nat(1), "{card}");
    assert_eq!(domain.members.len(), target_count, "{card}");
    let observation = game.observe(Viewer::P1).unwrap();
    assert_eq!(
        observation.processing.last().unwrap().stage,
        Some(ProcessingStage::Target),
        "{card}"
    );
}

#[test]
fn targeted_fanfares_cancel_without_legal_targets_after_the_card_enters() {
    for (card, line) in [
        ("BP01-006", 1),
        ("BP01-029", 2),
        ("BP01-038", 1),
        ("BP01-045", 1),
        ("BP01-047", 1),
        ("BP01-097", 1),
        ("BP01-155", 2),
        ("BP02-059", 2),
        ("BP02-094", 1),
        ("BP03-119", 2),
        ("BP10-119", 2),
        ("BP11-025", 2),
        ("BP12-108", 2),
        ("BP13-061", 2),
        ("BP14-033", 1),
        ("BP14-058", 1),
        ("BP15-111", 1),
        ("BP15-125", 2),
        ("BP18-068", 3),
        ("BP18-121", 2),
        ("BP18-124", 2),
        ("BP19-013", 3),
        ("BP20-068", 2),
        ("BP22-109", 2),
        ("CP02-077", 2),
        ("CSD02a-009", 3),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        field_entry(&mut game);
        assert_eq!(
            pending_play(&mut game, card, line),
            CardPlay::Cancelled,
            "{card}"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"],
            json!(["source"])
        );
    }
}

#[test]
fn terminal_weapon_enters_from_ex_without_playing_its_targetless_fanfare() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP10-T01"}]);
    let mut game = game(&initial);
    field_entry(&mut game);
    assert_eq!(pending_play(&mut game, "BP10-T01", 2), CardPlay::Cancelled);
}

#[test]
fn fanfares_still_declare_their_available_self_or_leader_target() {
    for (card, line) in [("BP01-094", 1), ("BP01-157", 2), ("BP15-115", 3)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        field_entry(&mut game);
        let play = pending_play(&mut game, card, line);
        declares_target(&game, &play, card, 1);
    }
}

#[test]
fn targetless_spells_leave_payment_hand_deck_and_life_untouched() {
    for card in ["BP14-017", "BP14-086", "BP15-093", "CP03-075", "ETD02-016"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"source","card":card},{"id":"reserve","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert!(
            matches!(
                game.play_internal_card("source", PLAYER),
                Err(EngineFailure::IllegalPlay(_))
            ),
            "{card}"
        );
        assert_eq!(game.projection(View::Referee).unwrap(), before, "{card}");
    }
}

#[test]
fn targetless_activations_pay_neither_resources_nor_source_posture() {
    for (card, line) in [
        ("BP01-006", 2),
        ("BP01-038", 2),
        ("BP11-091", 2),
        ("BP12-108", 3),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        let definition = selection().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.source.line == line)
            .unwrap()
            .node
            .id;
        let before = game.projection(View::Referee).unwrap();
        assert!(
            matches!(
                game.activate_internal_card("source", definition, PLAYER),
                Err(EngineFailure::IllegalPlay(_))
            ),
            "{card} line {line}"
        );
        assert_eq!(game.projection(View::Referee).unwrap(), before, "{card}");
    }
}

#[test]
fn bp05_059_declares_its_own_follower_before_resolving_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-059"}]);
    let mut game = game(&initial);
    let definition = game.internal_activated_sources("source").unwrap()[0].definition;
    let play = game
        .activate_internal_card("source", definition, PLAYER)
        .unwrap();
    declares_target(&game, &play, "BP05-059", 1);
}

fn evolve(game: &mut Game, form: &str, pp: u8) {
    let mut decision =
        json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}});
    if form == "BP19-083" {
        decision["definition"] = json!(ability("BP19-082", 0).0);
    }
    let result = game.internal_rule_action(&decision).unwrap();
    assert_eq!(result.outcome, "resolved", "{form}");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["evolved_with"],
        "form",
        "{form} is attached before its evolution trigger is played"
    );
}

#[test]
fn required_evolution_triggers_cancel_without_undoing_evolution() {
    for (base, form, pp, line) in [
        ("BP01-155", "BP01-156", 1, 1),
        ("BP01-159", "BP01-160", 2, 2),
        ("BP04-035", "BP04-036", 1, 2),
        ("BP04-112", "BP04-113", 1, 1),
        ("BP06-097", "BP06-098", 1, 2),
        ("BP07-014", "BP07-015", 1, 1),
        ("BP08-055", "BP08-056", 1, 1),
        ("BP09-029", "BP09-030", 4, 1),
        ("BP10-027", "BP10-028", 2, 2),
        ("BP12-080", "BP12-081", 2, 2),
        ("BP19-013", "BP19-014", 1, 2),
        ("BP19-060", "BP19-061", 1, 1),
        ("BP19-068", "BP19-069", 1, 3),
        ("BP19-075", "BP19-076", 5, 2),
        ("BP19-082", "BP19-083", 1, 1),
        ("BP20-064", "BP20-065", 1, 2),
        ("BP22-013", "BP22-014", 1, 1),
        ("BP22-031", "BP22-032", 1, 2),
        ("CP01-066", "CP01-067", 1, 2),
        ("CSD02b-010", "CSD02b-011", 2, 2),
        ("ECP02-067", "ECP02-068", 1, 1),
        ("ETD01-007", "ETD01-008", 1, 1),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut game = game(&initial);
        evolve(&mut game, form, pp);
        assert_eq!(
            pending_play(&mut game, form, line),
            CardPlay::Cancelled,
            "{form}"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["evolved_with"],
            "form"
        );
    }
}

#[test]
fn bp15_116_declares_its_own_evolved_follower_as_a_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP15-115"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-116"}]);
    let mut game = game(&initial);
    evolve(&mut game, "BP15-116", 1);
    let play = pending_play(&mut game, "BP15-116", 2);
    declares_target(&game, &play, "BP15-116", 1);
}

#[test]
fn an_attack_against_the_leader_cancels_targetless_attack_time_abilities() {
    for (base, form, card, line) in [
        ("BP08-027", None, "BP08-027", 2),
        ("BP11-083", None, "BP11-083", 2),
        ("BP12-012", None, "BP12-012", 3),
        ("BP19-068", None, "BP19-068", 3),
        ("BP19-068", Some("BP19-069"), "BP19-069", 2),
        ("BP20-017", None, "BP20-017", 2),
    ] {
        let mut initial = draw_position();
        let mut source = json!({"id":"source","card":base});
        if let Some(form) = form {
            source["state"] = json!({"evolved":true,"evolved_with":"form"});
            initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":form}]);
        }
        initial["players"]["P1"]["zones"]["field"] = json!([source]);
        let mut game = game(&initial);
        let attack = game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        assert_eq!(attack.outcome, "resolved", "{card}");
        assert_eq!(
            pending_play(&mut game, card, line),
            CardPlay::Cancelled,
            "{card}"
        );
    }
}

#[test]
fn lastwords_cancel_if_no_eligible_follower_remains_after_destruction() {
    for (card, line) in [
        ("BP01-045", 2),
        ("BP10-089", 2),
        ("BP12-P27", 2),
        ("BP13-074", 3),
        ("BP14-P29", 2),
    ] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        activate(&mut game, "P2");
        assert_eq!(
            pending_play(&mut game, card, line),
            CardPlay::Cancelled,
            "{card}"
        );
    }
}

#[test]
fn bp05_093_cancels_its_automatic_ability_when_the_opponent_starts_main_empty() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-093"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    assert_eq!(pending_play(&mut game, "BP05-093", 1), CardPlay::Cancelled);
}
