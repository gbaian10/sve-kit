#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed material scenarios."
)]
use super::*;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP19-047" | "BP05-057" | "CP02-T01" | "BP03-T02")));
        for (number, name, kind, power) in [
            ("power-single", "没頭の実験体", "フォロワー", "2"),
            ("power-multi", "多頭の実験体", "フォロワー", "3"),
            ("power-wrong", "別の実験体", "フォロワー", "1"),
            ("power-enemy", "enemy", "フォロワー", "1"),
            ("power-token", "token", "フォロワー・トークン", "3"),
            ("power-form", "没頭の実験体", "フォロワー・エボルヴ", "8"),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":[],"cost":if kind.contains("エボルヴ") {"-"} else {"1"},"power":power,"hp":"30","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn game(initial: &Value) -> Game {
    let mut board = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "paid-power",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "power-single",
        "power-multi",
        "power-wrong",
        "power-enemy",
        "power-token",
        "power-form",
    ] {
        board.load_internal_card(number, &blank).unwrap();
    }
    board
}

fn activation(number: &str) -> AbilityId {
    prepared().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn position(printing: &str) -> Value {
    let mut initial = draw_position();
    if printing.starts_with("BP19") {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP19-046","state":{"evolved":true,"evolved_with":"source-form","power":2_u8,"hp":2_u8,"max_hp":2_u8}}]);
        initial["players"]["P1"]["zones"]["evolution"] =
            json!([{"id":"source-form","card":printing}]);
    } else {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
    }
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"power-enemy","state":{"acted":true}}]);
    initial
}

fn finish(
    board: &mut Game,
    mut play: CardPlay,
    material: &str,
    leader: bool,
    candidates: usize,
) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                let selected = domain
                    .members
                    .iter()
                    .find(|member| {
                        if leader {
                            member.card.is_none()
                        } else {
                            member.card.as_deref() == Some("power-enemy")
                        }
                    })
                    .unwrap();
                Answer::Targets(vec![selected.handle])
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    candidates,
                    "only eligible own field materials are offered"
                );
                let selected = domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some(material))
                    .unwrap();
                Answer::CostMaterials(vec![selected.handle])
            }
            _ => panic!("unexpected paid material question"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

#[test]
fn researcher_both_printings_accept_either_exact_experiment_name_and_keep_its_modified_attack() {
    for (printing, chosen, attack) in [
        ("BP19-047", "power-single", 9_u8),
        ("BP19-P15", "power-multi", 7_u8),
    ] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"single","card":"power-single","state":{"power":9_u8}}),
                json!({"id":"multi","card":"power-multi","state":{"power":7_u8}}),
                json!({"id":"wrong","card":"power-wrong"}),
            ]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"foreign","card":"power-single"}));
        let mut board = game(&initial);
        let play = board
            .activate_internal_card("source", activation(printing), PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut board, play, chosen, false, 2));
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"], attack,
            "the complete printed activation uses the paid experiment's actual attack"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if chosen == "power-single" {
                json!(["single"])
            } else {
                json!(["multi"])
            },
            "one chosen experiment actually leaves the field"
        );
        assert_eq!(
            state["objects"]["source"]["acted"], true,
            "the researcher pays its posture first"
        );
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("wrong")),
            "a partial name cannot pay"
        );
        assert!(
            state["P2"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("foreign")),
            "opposing experiments cannot pay"
        );
    }
}

#[test]
fn god_bullet_reads_modified_evolved_and_token_materials_and_can_damage_a_leader() {
    for (material, attack, evolved, leader, damage) in [
        ("power-single", 9_i64, false, false, 9_i64),
        ("power-single", 8, true, false, 8),
        ("power-token", 3, false, true, 3),
        ("power-single", -2, false, false, 0),
    ] {
        let mut initial = position("BP05-057");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":material,"state":{"power":attack}}));
        if evolved {
            initial["players"]["P1"]["zones"]["field"][1]["state"] = json!({"evolved":true,"evolved_with":"form","power":8_u8,"hp":30_u8,"max_hp":30_u8});
            initial["players"]["P1"]["zones"]["evolution"] =
                json!([{"id":"form","card":"power-form"}]);
        }
        let mut board = game(&initial);
        let play = board
            .activate_internal_card("source", activation("BP05-057"), PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut board, play, material, leader, 1));
        let state = board.projection(View::Referee).unwrap();
        if leader {
            assert_eq!(
                state["P2"]["leader"]["life"], 17_u8,
                "the paid token damages the chosen leader"
            );
            assert!(
                state["objects"].get("material").is_none(),
                "the real token erasure does not erase its payment information"
            );
        } else {
            assert_eq!(
                state["objects"]["enemy"]["damage"], damage,
                "Q715/Q716 use the evolved or modified attack before leaving"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["material"]),
                "the material is put into the cemetery"
            );
        }
        if evolved {
            assert_eq!(
                state["P1"]["evolve_deck"],
                json!(["form"]),
                "the paid evolved follower returns its physical form"
            );
        }
    }
}

#[test]
fn god_bullet_permanently_forbids_only_leader_attacks_and_still_attacks_followers() {
    for (target, expected) in [("P2.leader", "cannot-attack"), ("enemy", "resolved")] {
        let mut board = game(&position("BP05-057"));
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
                .unwrap()
                .outcome,
            expected,
            "Q713 distinguishes leader and follower attacks"
        );
    }
}

#[test]
fn researcher_does_not_act_or_sacrifice_without_a_legal_enemy_target() {
    let mut initial = position("BP19-047");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"material","card":"power-single"}));
    let mut board = game(&initial);
    let before = board.projection(View::Referee).unwrap();
    let error = board
        .activate_internal_card("source", activation("BP19-047"), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(_)),
        "the known activation fails for legality, not for an unknown definition"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "no target means no payment"
    );
}
