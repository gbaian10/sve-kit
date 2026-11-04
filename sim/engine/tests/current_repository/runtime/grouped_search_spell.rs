#![expect(
    clippy::panic,
    reason = "Complete printed grouped-search scenarios require the declared domains."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-grouped-search",
    )
    .unwrap()
}

fn position(number: &str, zone: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("the complete printed ability requires a decision")
    };
    q
}

fn keyword(view: &Value, id: &str, name: &str) -> bool {
    view["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}

fn pending(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability")
    };
    let entry = options
        .iter()
        .find(|entry| entry.card == number && entry.line == line)
        .unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(entry.handle))
        .unwrap()
}

fn search(board: &mut Game, picks: [bool; 2]) -> CardPlay {
    let q = awaiting(
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "opponent cannot see private search handles"
    );
    let InputDomain::Search(domain) = &q.domain else {
        panic!("same grouped search")
    };
    let names = ["BP09-027", "BP09-028"];
    let groups = domain.groups.as_ref().unwrap();
    assert_eq!(groups.len(), 2, "the two named cards belong to one Search");
    let answers = names
        .iter()
        .zip(picks)
        .map(|(number, take)| {
            if !take {
                return vec![];
            }
            domain
                .cards
                .iter()
                .find(|card| card.card == *number)
                .map(|card| vec![card.handle])
                .unwrap_or_default()
        })
        .collect();
    board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(answers)),
        )
        .unwrap()
}

#[test]
#[expect(clippy::too_many_lines, reason = "One printed interaction trace verifies the shared entry, real helpers and subsequent attack without resetting the board.")]
fn both_printings_buff_actual_entries_and_preserve_the_complete_imera_magnus_interaction() {
    for number in ["BP09-024", "BP09-P08"] {
        let mut initial = position(number, "hand", 5);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"old","card":"BP09-027"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"imera","card":"BP09-027"},{"id":"magnus","card":"BP09-028"},{"id":"left","card":"BP09-027"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"acted":false,"power":0_u8,"hp":20_u8,"max_hp":20_u8}}
        ]);
        let mut board = game(&initial);
        let play = search(&mut board, [true, true]);
        resolved(play);
        let state_1 = board.projection(View::Referee).unwrap();
        assert_eq!(
            state_1["P1"]["pp"]["current"], 0_u8,
            "normal play pays exactly the Spell's printed five PP"
        );
        assert_eq!(
            state_1["objects"]["imera"]["power"], 5_u8,
            "new Imera receives the Search buff"
        );
        assert_eq!(
            state_1["objects"]["imera"]["hp"], 4_u8,
            "new Imera receives the Search life"
        );
        assert_eq!(
            state_1["objects"]["magnus"]["power"], 3_u8,
            "new Magnus receives the Search buff"
        );
        assert_eq!(
            state_1["objects"]["magnus"]["hp"], 3_u8,
            "new Magnus receives the Search life"
        );
        for id in ["old", "left"] {
            assert_eq!(
                state_1["objects"][id]["power"], 4_u8,
                "old or unselected same-name Imera is not buffed"
            );
            assert_eq!(
                state_1["objects"][id]["hp"], 3_u8,
                "old or unselected same-name Imera keeps printed life"
            );
        }
        assert!(keyword(&state_1, "imera", "突進"), "complete Imera keeps Rush");
        assert!(
            keyword(&state_1, "imera", "指定攻撃") && keyword(&state_1, "old", "指定攻撃"),
            "complete Magnus grants Assail to new and old Imera"
        );
        resolved(pending(&mut board, "BP09-027", 3));
        finish_checks(&mut board);
        let state_2 = board.projection(View::Referee).unwrap();
        let knights = state_2["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| state_2["objects"][id.as_str().unwrap()]["name"] == "ナイト")
            .count();
        assert_eq!(
            knights, 2,
            "Imera's real Fanfare sees Magnus from the same entry and makes two Knights"
        );
        let ability = board
            .internal_activated_sources("magnus")
            .unwrap()
            .remove(0);
        let q = awaiting(
            board
                .activate_internal_card_instance("magnus", ability.instance, PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("Magnus's real upto-two targets")
        };
        assert!(
            !domain
                .members
                .iter()
                .any(|member| member.card.as_deref() == Some("BP09-028")),
            "Magnus cannot choose itself"
        );
        let chosen = domain
            .members
            .iter()
            .filter(|member| member.card.as_deref() == Some("BP09-027"))
            .map(|member| member.handle)
            .collect();
        resolved(
            board
                .answer_internal_card(q.actor, q.token, Answer::Targets(chosen))
                .unwrap(),
        );
        finish_checks(&mut board);
        let state_3 = board.projection(View::Referee).unwrap();
        assert_eq!(
            state_3["objects"]["imera"]["power"], 6_u8,
            "Magnus's complete activation adds its separate power buff"
        );
        assert_eq!(
            state_3["objects"]["magnus"]["acted"], true,
            "Magnus pays its printed ActSelf cost"
        );
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"attack","attacker":"imera","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "new Imera can attack an unacted follower using Rush and Assail"
        );
        resolved(pending(&mut board, "BP09-027", 2));
        let state_4 = board.projection(View::Referee).unwrap();
        assert_eq!(
            state_4["P1"]["ex"].as_array().unwrap().len(),
            1,
            "Imera's complete attack trigger creates a Knight in EX"
        );
    }
}

#[test]
fn both_printings_may_find_only_magnus_or_neither_even_with_imera_in_the_deck() {
    for number in ["BP09-024", "BP09-P08"] {
        for picks in [[false, true], [false, false]] {
            let mut initial = position(number, "hand", 5);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"imera","card":"BP09-027"},{"id":"magnus","card":"BP09-028"}
            ]);
            let mut board = game(&initial);
            let play = search(&mut board, picks);
            resolved(play);
            finish_checks(&mut board);
            let state_5 = board.projection(View::Referee).unwrap();
            assert_eq!(
                state_5["P1"]["field"].as_array().unwrap().len(),
                usize::from(picks[1]),
                "Q1165 allows not finding an available named card"
            );
            assert_eq!(
                state_5["objects"]["imera"]["power"], 4_u8,
                "declined Imera stays unmodified"
            );
            assert_eq!(
                state_5["objects"]["magnus"]["power"],
                if picks[1] { 3_u8 } else { 2_u8 },
                "only an actual Magnus entry receives the buff"
            );
        }
    }
}

#[test]
fn complete_cemetery_activation_pays_two_banishes_itself_and_changes_only_own_knights() {
    for number in ["BP09-024", "BP09-P08"] {
        for (zone, pp, allowed) in [
            ("cemetery", 2, true),
            ("cemetery", 1, false),
            ("hand", 2, false),
            ("ex", 2, false),
        ] {
            let mut initial = position(number, zone, pp);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"knight","card":"BP17-T11"},{"id":"other","card":"BP09-028"}
            ]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"foreign","card":"BP17-T11"}]);
            let mut board = game(&initial);
            let ability = board
                .internal_activated_sources("source")
                .unwrap()
                .remove(0);
            let before = format!("{board:?}");
            let result = board.activate_internal_card_instance(
                "source",
                ability.instance,
                PlayerId { seat: 1 },
            );
            if !allowed {
                result.unwrap_err();
                assert_eq!(
                    format!("{board:?}"),
                    before,
                    "insufficient PP or wrong active zone restores the whole attempt"
                );
                continue;
            }
            resolved(result.unwrap());
            finish_checks(&mut board);
            let state_6 = board.projection(View::Referee).unwrap();
            assert_eq!(
                state_6["P1"]["pp"]["current"], 0_u8,
                "the cemetery ability pays two PP"
            );
            assert!(
                state_6["P1"]["banish"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                "the cost banishes this actual Spell"
            );
            assert_eq!(
                state_6["objects"]["knight"]["power"], 2_u8,
                "own Knight gains power"
            );
            assert!(keyword(&state_6, "knight", "疾走"), "own Knight gains Storm");
            assert_eq!(
                state_6["objects"]["foreign"]["power"], 1_u8,
                "opposing Knight is excluded"
            );
            assert!(
                !keyword(&state_6, "foreign", "疾走"),
                "opposing Knight does not gain Storm"
            );
            assert_eq!(
                state_6["objects"]["other"]["power"], 2_u8,
                "other names are excluded"
            );
            assert!(
                !keyword(&state_6, "other", "疾走"),
                "non-Knight does not gain Storm"
            );
        }
    }
}

#[test]
fn real_grouped_search_only_buffs_the_capacity_choice_and_accepts_a_missing_named_group() {
    for number in ["BP09-024", "BP09-P08"] {
        for (free, missing) in [(0_u8, false), (1, false), (2, true)] {
            let mut initial = position(number, "hand", 5);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..(5 - free))
                    .map(|n| json!({"id":format!("old{n}"),"card":"BP01-041"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"magnus","card":"BP09-028"}]);
            if !missing {
                initial["players"]["P1"]["zones"]["deck"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"imera","card":"BP09-027"}));
            }
            let mut board = game(&initial);
            let play = search(&mut board, [true, true]);
            let play = if let CardPlay::Awaiting(q) = play {
                let InputDomain::SearchPartition(domain) = &q.domain else {
                    panic!("one free Field space")
                };
                assert_eq!(
                    free, 1,
                    "only the partial-capacity case asks for an allocation"
                );
                let chosen = domain
                    .cards
                    .iter()
                    .find(|card| card.card == "BP09-028")
                    .unwrap();
                board
                    .answer_internal_card(
                        q.actor,
                        q.token,
                        Answer::SearchPartition(vec![chosen.handle]),
                    )
                    .unwrap()
            } else {
                play
            };
            resolved(play);
            finish_checks(&mut board);
            let state_7 = board.projection(View::Referee).unwrap();
            assert_eq!(
                state_7["objects"]["magnus"]["power"],
                if free == 0 { 2_u8 } else { 3_u8 },
                "only successful Magnus entry receives the buff"
            );
            assert_eq!(
                state_7["objects"]["magnus"]["hp"],
                if free == 0 { 2_u8 } else { 3_u8 },
                "failed movement produces no life modification"
            );
            if !missing {
                assert_eq!(
                    state_7["objects"]["imera"]["power"], 4_u8,
                    "found but unentered Imera receives no modification"
                );
            }
            assert!(
                board.internal_pending_choice().unwrap().is_none(),
                "unentered or missing Imera produces no Fanfare"
            );
            assert_eq!(
                state_7["P1"]["field"].as_array().unwrap().len(),
                usize::from(5 - free + u8::from(free > 0)),
                "only the actual capacity choice enters"
            );
        }
    }
}
