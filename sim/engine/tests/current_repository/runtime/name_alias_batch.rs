#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate these complete printed-card scenarios."
)]
use super::*;

fn finish(board: &mut Game, mut play: CardPlay) {
    for _ in 0..16_u8 {
        while let CardPlay::Awaiting(q) = play {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .take(
                            d.cardinality
                                .maximum
                                .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap()),
                        )
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                _ => panic!("unexpected printed choice"),
            };
            play = board
                .answer_internal_card(q.actor, q.token, answer)
                .unwrap();
        }
        event_cemetery::finish_checks(board);
        let Some(q) = board.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending");
        };
        play = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
    }
    panic!("printed pending chain did not settle");
}

fn settle(board: &mut Game) {
    event_cemetery::finish_checks(board);
    let Some(q) = board.internal_pending_choice().unwrap() else {
        return;
    };
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending");
    };
    let play = board
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(board, play);
}

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve(board: &mut Game, base: &str, super_evolution: bool) {
    let definition = selection().current[base]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let mut pay = json!({"pp":1_u8});
    if super_evolution {
        pay["sep"] = json!(1_u8);
    }
    assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":definition.0,"pay":pay})).unwrap().outcome, "resolved", "the selected printed evolution resolves with its declared payment");
    settle(board);
}

#[test]
fn zwei_every_printing_transforms_an_ex_puppet_into_complete_victoria() {
    for number in ["BP08-004", "BP08-P01", "SP01-004", "SP01-SL04"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"puppet","card":"BP08-T09"},
            {"id":"lloyd","card":"BP08-T01"}
        ]);
        let mut board = game(&initial);
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        event_cemetery::finish_checks(&mut board);
        let pending = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("Zwei fanfare");
        };
        assert_eq!(options.len(), 1);
        let trigger_play = board
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(target) = trigger_play else {
            panic!("Zwei target");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("Zwei target domain");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "Lloyd's field-only alias is inactive in EX"
        );
        assert_eq!(domain.members[0].card.as_deref(), Some("BP08-T09"));
        let play = board
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        finish(&mut board, play);
        let state = board.projection(View::Referee).unwrap();
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 2);
        assert!(ex.contains(&json!("lloyd")));
        assert_eq!(state["objects"]["lloyd"]["name"], "ロイド");
        let victoria_id = ex
            .iter()
            .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "ヴィクトリア")
            .unwrap()
            .as_str()
            .unwrap();
        let victoria = &state["objects"][victoria_id];
        assert_eq!(victoria["power"], 4_u8);
        assert_eq!(victoria["hp"], 1_u8);
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert_eq!(state["P1"]["hand"], json!([]));
    }
}

#[test]
fn evolved_bakels_every_printing_returns_the_base_to_ex_and_the_form_to_evolve_deck() {
    for number in ["BP03-079", "ETD02-002"] {
        let initial = position("BP03-078", number);
        let action = event_cemetery::move_named(
            ZoneKind::Field,
            OwnerZone::Cemetery,
            "プチゴースト・バケルス",
        );
        let mut board = with_producer(&initial, action);
        evolve(&mut board, "BP03-078", false);
        activate(&mut board, "P1");
        settle(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["source"]));
        assert_eq!(state["P1"]["evolve_deck"], json!(["form"]));
        assert_eq!(state["objects"]["form"]["face_up"], true);
    }
}

#[test]
fn attack_mode_every_printing_damages_the_selected_follower_and_its_leader_on_attack() {
    for number in ["BP03-058", "BP03-SL13"] {
        let mut initial = position("BP03-056", number);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":30_u8,"max_hp":30_u8}}]);
        let mut board = game(&initial);
        evolve(&mut board, "BP03-056", false);
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        settle(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    }
}

#[test]
fn defense_mode_every_printing_heals_at_its_controllers_end_phase() {
    for number in ["BP04-061", "BP04-SL13"] {
        let mut initial = position("BP03-056", number);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut board = game(&initial);
        evolve(&mut board, "BP03-056", false);
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}

#[test]
fn blast_mode_every_printing_counts_armed_followers_when_evolution_resolves() {
    for number in ["BP04-062", "BP04-SL14"] {
        let mut initial = position("BP03-056", number);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"armed","card":"BP03-060"}));
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}}]);
        let mut board = game(&initial);
        evolve(&mut board, "BP03-056", false);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            4_u8
        );
    }
}

#[test]
fn lyle_every_printing_creates_its_doll_and_super_evolution_buffs_the_ex_token() {
    for number in ["BP21-003", "BP21-SL03"] {
        let mut initial = position("BP21-002", number);
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
        initial["turn"]["elapsed_turns"]["P2"] = json!(6_u8);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"g1","card":"BP21-002"},{"id":"g2","card":"BP16-003"},{"id":"g3","card":"BP19-004"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"doll","card":"BP21-P63"}]);
        let mut board = game(&initial);
        evolve(&mut board, "BP21-002", true);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["doll"]["power"], 5_u8);
        assert_eq!(state["objects"]["doll"]["max_hp"], 5_u8);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn lyles_doll_every_printing_enters_with_all_keywords_and_can_attack_immediately() {
    for number in ["BP21-P63", "BP21-T01"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"doll","card":number}]);
        let mut board = game(&initial);
        let play = board
            .play_internal_card("doll", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play);
        let state = board.projection(View::Referee).unwrap();
        for keyword in ["疾走", "必殺", "守護"] {
            assert!(
                state["objects"]["doll"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword))
            );
        }
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"doll","target":"P2.leader"}))
            .unwrap();
        let q = board.internal_quick_choice().unwrap().unwrap();
        board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            19_u8
        );
    }
}
