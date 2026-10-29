use super::*;

fn commander_game(initial: &Value, kind: &str, command: bool) -> Game {
    let (mut snapshot, keywords, mut input) = inputs().clone();
    let mut face = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|card| card["number"] == "BP01-037")
        .unwrap()["faces"][0]
        .clone();
    face["name"] = json!("commander-companion");
    face["card_type"] = json!(kind);
    face["traits"] = if command {
        json!(["指揮官"])
    } else {
        json!(["兵士"])
    };
    if kind == "アミュレット" {
        face["power"] = json!("-");
        face["hp"] = json!("-");
    }
    face["text"] = Value::Null;
    face["sections"] = json!([]);
    snapshot.push_str(&json!({"number":"commander-companion","faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!(["commander-companion.yaml", "format: 1\nkind: effect_set\ncards:\n  commander-companion:\n    status: complete\n    abilities: []\n"]));
    let mut game = game_from_selection(initial, &snapshot, &keywords, &input);
    game.load_internal_card(
        "commander-companion",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn position(seat: &str, owner: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["field"] = json!([{"id":"source","card":"BP08-032"}]);
    initial["players"][seat]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-033"}]);
    if initial["players"][owner]["zones"][zone].is_null() {
        initial["players"][owner]["zones"][zone] = json!([]);
    }
    initial["players"][owner]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"commander","card":"commander-companion"}));
    initial
}

fn decision(line: u32, pp: u8) -> Value {
    let definition = selection().current["BP08-032"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    json!({"do":"evolve","source":"source","evolve_card":"form","definition":definition.0,"pay":{"pp":pp}})
}

#[test]
fn commander_evolution_bp08_032_requires_a_commander_card_on_its_own_field() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for (owner, zone, kind, commander, allowed) in [
            (seat, "field", "フォロワー", true, true),
            (seat, "field", "アミュレット", true, true),
            (opponent, "field", "フォロワー", true, false),
            (seat, "hand", "フォロワー", true, false),
            (seat, "ex", "フォロワー", true, false),
            (seat, "cemetery", "フォロワー", true, false),
            (seat, "field", "フォロワー", false, false),
        ] {
            let mut game = commander_game(&position(seat, owner, zone), kind, commander);
            let before = game.projection(View::Referee).unwrap();
            let result = game.internal_rule_action(&decision(2, 0));
            if allowed {
                result.unwrap();
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state[seat]["pp"]["current"], 10_u8,
                    "the qualified second ability costs zero"
                );
                assert_eq!(
                    state["objects"]["source"]["power"], 6_u8,
                    "the actual evolved card is installed"
                );
            } else {
                assert_eq!(
                    result.unwrap().outcome,
                    "cannot-evolve",
                    "the free definition is not usable"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "an illegal free evolution cannot reveal or move the resource or spend PP"
                );
                game.internal_rule_action(&decision(1, 2)).unwrap();
                assert_eq!(
                    game.projection(View::Referee).unwrap()[seat]["pp"]["current"],
                    8_u8,
                    "the ordinary evolution remains legal and the failed attempt did not consume the quota"
                );
            }
        }
    }
}

#[test]
fn commander_evolution_bp08_033_deals_six_after_either_printed_evolution() {
    for (line, pp) in [(1_u32, 2_u8), (2, 0)] {
        let mut initial = position("P1", "P1", "field");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
        let mut game = commander_game(&initial, "アミュレット", true);
        game.internal_rule_action(&decision(line, pp)).unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("evolution trigger");
        };
        assert_eq!(
            options[0].card, "BP08-033",
            "the real form supplies the trigger"
        );
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("formal target");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("follower target");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only the opponent's follower is selectable"
        );
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"], 6_u8,
            "the printed evolution effect deals six"
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            10_u8 - pp,
            "each definition pays its own printed cost"
        );
    }
}
