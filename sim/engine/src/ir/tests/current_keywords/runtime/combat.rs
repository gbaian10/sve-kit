//! Real printings reach combat through C0 selection and normal rule decisions.
use super::*;

const STORM: &str = "BP01-158";
const RUSH: &str = "BP09-114";
const ASSAIL: &str = "BP01-163";
const INTIMIDATE: &str = "BP01-096";
const BANE: &str = "BP03-033";

fn selection() -> Value {
    let mut selection = current_inputs()["keywords"].clone();
    for entry in current_inputs()["combat_keywords"]
        .as_object()
        .unwrap()
        .values()
    {
        selection["authors"]
            .as_array_mut()
            .unwrap()
            .push(entry["authors"][0].clone());
    }
    selection
}

fn game(initial: &Value) -> Game {
    selected_game(initial, selection(), &snapshot(), &producer(vec![], None))
}

fn attack(game: &mut Game, source: &str, target: &str) -> String {
    game.internal_rule_action(&json!({"do":"attack","attacker":source,"target":target}))
        .unwrap()
        .outcome
}

fn battle(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn real_combat_printings_preserve_every_source_and_sibling_before_admission() {
    let lowerer = lowering::ExpandedLowerer::new().unwrap();
    for (number, entry) in current_inputs()["combat_keywords"].as_object().unwrap() {
        let input = &entry["authors"][0]["input"];
        let candidate = lowerer.lower_current(input).unwrap();
        let declarations = input["declaration_sidecar"].as_array().unwrap();
        let count: usize = declarations
            .iter()
            .map(|d| d["declaration"]["keywords"].as_array().unwrap().len())
            .sum();
        assert_eq!(
            candidate.program.card().abilities.len(),
            count + input["sidecar"].as_array().unwrap().len(),
            "{number}"
        );
        for ability in &candidate.program.card().abilities {
            if let AbilityOrigin::Keyword { keyword, origin } = &ability.node.source.origin {
                if *keyword == KeywordId::Evolution {
                    continue;
                }
                assert!(
                    matches!(origin.as_ref(), AbilityOrigin::Printed {face, line}
                    if face.0 == ability.source.face && line.0 == u64::from(ability.source.line))
                );
                assert!(
                    candidate
                        .declaration_at(&ability.source.author_path)
                        .is_some()
                );
            }
        }
        super::super::admission(entry).unwrap();
    }
}

#[test]
fn storm_and_rush_use_normal_play_and_preserve_posture_and_target_restrictions() {
    // CR 12.9.2 / 12.10.2; FAQ Q30-Q31: Rush never permits a same-turn leader attack.
    for seat in ["P1", "P2"] {
        let other = opponent(seat);
        for number in [STORM, RUSH] {
            for zone in ["hand", "ex"] {
                for target in [format!("{other}.leader"), "acted".into(), "standing".into()] {
                    let mut initial = setup(seat);
                    initial["players"][seat]["zones"][zone] =
                        json!([{"id":"incoming","card":number}]);
                    initial["players"][other]["zones"]["field"] = json!([
                        {"id":"acted","card":"plain","state":{"acted":true}},
                        {"id":"standing","card":"plain"}
                    ]);
                    let mut game = game(&initial);
                    assert!(matches!(
                        game.play_internal_card("incoming", actor(seat)).unwrap(),
                        CardPlay::Entered(_)
                    ));
                    game.resume_internal_card().unwrap();
                    let before = views(&game);
                    let allowed =
                        target == "acted" || (number == STORM && target.ends_with(".leader"));
                    assert_eq!(
                        attack(&mut game, "incoming", &target),
                        if allowed { "resolved" } else { "cannot-attack" }
                    );
                    if allowed {
                        assert_eq!(state(&game, "incoming")["acted"], true);
                        battle(&mut game);
                        assert_eq!(attack(&mut game, "incoming", &target), "cannot-attack");
                    } else {
                        assert_eq!(views(&game), before);
                    }
                }
            }
        }
    }
}

#[test]
fn combat_keywords_obey_ability_loss_and_controller() {
    // CR 12.9-12.12: controller governs attacks, while lost abilities confer no exception.
    for seat in ["P1", "P2"] {
        let other = opponent(seat);
        for (number, target, entered) in [
            (STORM, format!("{other}.leader"), true),
            (RUSH, "target".into(), true),
            (ASSAIL, "standing".into(), false),
        ] {
            for silenced in [false, true] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] = json!([{"id":"attacker","card":number,"owner":other,
                    "state":{"entered_this_turn":entered,"silenced":silenced}}]);
                initial["players"][other]["zones"]["field"] = json!([
                    {"id":"target","card":"plain","state":{"acted":true}},
                    {"id":"standing","card":"plain"}
                ]);
                let mut game = game(&initial);
                assert_eq!(
                    attack(&mut game, "attacker", &target),
                    if silenced {
                        "cannot-attack"
                    } else {
                        "resolved"
                    }
                );
            }
        }
        // Assail changes targets, not the same-turn attack restriction.
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] =
            json!([{"id":"attacker","card":ASSAIL,"state":{"entered_this_turn":true}}]);
        initial["players"][other]["zones"]["field"] = json!([{"id":"target","card":"plain"}]);
        assert_eq!(
            attack(&mut game(&initial), "attacker", "target"),
            "cannot-attack"
        );
    }
}

#[test]
fn evolution_state_does_not_replace_storm_for_same_turn_leader_attacks() {
    // CR 8.4.2.1 / 8.4.3.1: probe an initial evolution state, not an evolution action.
    for seat in ["P1", "P2"] {
        for number in [STORM, RUSH] {
            for target in [format!("{}.leader", opponent(seat)), "target".into()] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] = json!([{
                    "id":"attacker","card":number,
                    "state":{"entered_this_turn":true,"evolved":true}
                }]);
                initial["players"][opponent(seat)]["zones"]["field"] =
                    json!([{"id":"target","card":"plain","state":{"acted":true}}]);
                let mut game = game(&initial);
                let allowed = number == STORM || target == "target";
                assert_eq!(
                    attack(&mut game, "attacker", &target),
                    if allowed { "resolved" } else { "cannot-attack" }
                );
            }
        }
    }
}

#[test]
fn assail_obeys_ward_and_intimidate_but_can_attack_aura() {
    // CR 12.11.2 / 12.12.2; FAQ Q325: Assail never bypasses an attack restriction.
    for seat in ["P1", "P2"] {
        for (number, acted, allowed) in [
            (INTIMIDATE, false, false),
            (INTIMIDATE, true, false),
            (NUMBER, false, true),
        ] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] = json!([{"id":"attacker","card":ASSAIL}]);
            initial["players"][opponent(seat)]["zones"]["field"] =
                json!([{"id":"target","card":number,"state":{"acted":acted}}]);
            assert_eq!(
                attack(&mut game(&initial), "attacker", "target"),
                if allowed { "resolved" } else { "cannot-attack" }
            );
        }
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"attacker","card":ASSAIL}]);
        initial["players"][opponent(seat)]["zones"]["field"] = json!([
            {"id":"ward","card":NUMBER,"state":{"acted":true}}, {"id":"other","card":"plain"}
        ]);
        assert_eq!(
            attack(&mut game(&initial), "attacker", "other"),
            "cannot-attack"
        );
        assert_eq!(attack(&mut game(&initial), "attacker", "ward"), "resolved");
    }
}

#[test]
fn intimidate_does_not_protect_against_play_targets_or_survive_ability_loss() {
    // CR 12.12.2.1: Intimidate restricts attacks, not a spell/ability target domain.
    for seat in ["P1", "P2"] {
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
        initial["players"][opponent(seat)]["zones"]["field"] =
            json!([{"id":"target","card":INTIMIDATE,"owner":seat}]);
        let program = producer(
            vec![Action::Destroy {
                objects: Selector::Bound {
                    reference: Binding::new(10, ScopeId(1)),
                },
            }],
            Some(PlayerRelation::Opponents),
        );
        let mut game = selected_game(&initial, selection(), &snapshot(), &program);
        let q = question(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("target question")
        };
        assert_eq!(domain.members.len(), 1);
        initial["players"][opponent(seat)]["zones"]["field"][0]["state"] =
            json!({"acted":true,"silenced":true});
        assert_eq!(
            attack(&mut self::game(&initial), "producer", "target"),
            "resolved"
        );
    }
}

#[test]
fn bane_destroys_after_battle_even_with_zero_power_in_either_direction() {
    // CR 12.14.2-12.14.2.1 / 11.3.2; FAQ Q35: battle, not positive damage, is required.
    for seat in ["P1", "P2"] {
        for defending in [false, true] {
            for silenced in [false, true] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] = json!([{"id":"attacker","card":if defending {"plain"} else {BANE},
                    "state":{"power":0_i64,"hp":20_i64,"silenced":silenced}}]);
                initial["players"][opponent(seat)]["zones"]["field"] = json!([{"id":"defender","card":if defending {BANE} else {"plain"},
                    "state":{"power":0_i64,"hp":20_i64,"acted":true,"silenced":silenced}}]);
                let mut game = game(&initial);
                assert_eq!(attack(&mut game, "attacker", "defender"), "resolved");
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(view[seat]["field"], json!(["attacker"]));
                assert_eq!(view[opponent(seat)]["field"], json!(["defender"]));
                assert_eq!(state(&game, "attacker")["power"], 0_i64);
                assert_eq!(state(&game, "defender")["power"], 0_i64);
                battle(&mut game);
                let victim = if defending { "attacker" } else { "defender" };
                let owner = if defending { seat } else { opponent(seat) };
                assert_eq!(
                    game.projection(View::Referee).unwrap()[owner]["cemetery"],
                    if silenced { json!([]) } else { json!([victim]) }
                );
                if !silenced {
                    assert_eq!(state(&game, victim)["generation"], 1_u8);
                }
            }
        }
    }
}

#[test]
fn every_combat_keyword_requires_its_effective_face_field_and_abilities() {
    // CR 4.1.4 / 10.3.5: the synthetic blank reverse has no printed keywords.
    for (number, keyword) in [
        (STORM, "疾走"),
        (RUSH, "突進"),
        (ASSAIL, "指定攻撃"),
        (INTIMIDATE, "威圧"),
        (BANE, "必殺"),
    ] {
        let (selection, snapshot) =
            synthetic_reverse_for(number, current_inputs()["combat_keywords"][number].clone());
        for seat in ["P1", "P2"] {
            for (face, silenced, zone) in [
                (0_u8, false, "field"),
                (1, false, "field"),
                (0, true, "field"),
                (0, false, "cemetery"),
            ] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"][zone] = json!([{"id":"subject","card":number,"state":{"face":face,"silenced":silenced}}]);
                let game = selected_game(
                    &initial,
                    selection.clone(),
                    &snapshot,
                    &producer(vec![], None),
                );
                let expected = if face == 0 && !silenced && zone == "field" {
                    json!([keyword])
                } else {
                    json!([])
                };
                assert_eq!(state(&game, "subject")["keywords"], expected, "{number}");
            }
        }
    }
}

#[test]
fn combat_names_do_not_admit_unproven_partial_or_nested_definitions() {
    use super::super::{raw_keyword_coordinates, raw_keyword_game_for};
    let lowerer = lowering::ExpandedLowerer::new().unwrap();
    for number in [STORM, RUSH, ASSAIL, INTIMIDATE, BANE] {
        let input = &current_inputs()["combat_keywords"][number]["authors"][0]["input"];
        let candidate = lowerer.lower_current(input).unwrap();
        let mut raw = raw_keyword_coordinates(candidate.program.card());
        // The raw-loader probe isolates the declaration; complete real siblings are tested above.
        raw.abilities.truncate(1);
        let original = WellFormedCardProgram::check(raw.clone()).unwrap();
        let failure = raw_keyword_game_for(number)
            .load_internal_card(number, &original)
            .unwrap_err();
        assert!(
            matches!(failure, crate::EngineFailure::Unsupported(reason) if reason.contains("requires current declaration proof"))
        );
        let mut variants = Vec::new();
        let mut nested = raw.clone();
        if let AbilityOrigin::Keyword { origin, .. } = &mut nested.abilities[0].node.source.origin {
            **origin = candidate.program.card().abilities[0]
                .node
                .source
                .origin
                .clone();
        }
        variants.push(nested);
        let mut zone = raw.clone();
        zone.abilities[0].node.active_zones = vec![ZoneKind::Hand];
        variants.push(zone);
        let mut holder = raw.clone();
        holder.abilities[0].node.source.holder = ObjectOperand::Bound {
            binding: Binding::new(1, holder.abilities[0].node.scope),
        };
        variants.push(holder);
        let mut owner = raw.clone();
        owner.abilities[0].node.source.controller = PlayerRef::Owner {};
        variants.push(owner);
        let mut foreign = raw;
        if let AbilityOrigin::Keyword { keyword, .. } = &mut foreign.abilities[0].node.source.origin
        {
            *keyword = KeywordId::Drain;
        }
        variants.push(foreign);
        for variant in variants {
            if let Ok(checked) = WellFormedCardProgram::check(variant) {
                raw_keyword_game_for(number)
                    .load_internal_card(number, &checked)
                    .unwrap_err();
            }
        }
        let mut missing = input.clone();
        missing["tree"]["cards"][number]["abilities"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        missing["declaration_sidecar"] = json!([]);
        for sidecar in missing["sidecar"].as_array_mut().unwrap() {
            sidecar["path"][3] = json!(0_u8);
        }
        assert_eq!(
            lowerer.lower_current(&missing).unwrap_err().code,
            "current_keyword_closure_mismatch"
        );
    }
}

fn synthetic_combination(keywords: &[&str], text: &str) -> (Value, String) {
    // A D0 trusted-handoff probe, not a claim about BP01-096's real printed text.
    let hash = text_hash(text);
    let mut selection = current_inputs()["combat_keywords"][INTIMIDATE].clone();
    let input = &mut selection["authors"][0]["input"];
    input["tree"]["cards"][INTIMIDATE]["abilities"][0]["keywords"] = json!(keywords);
    input["source_payload"]["faces"][0]["texts"][1]["source_hash"] = json!(hash);
    let mut source = input["declaration_sidecar"][0]["source_use"].clone();
    source["occurrence"]["source_hash"] = json!(hash);
    source["occurrence"]["segments"] = json!([{"start":0_usize,"end":text.chars().count()}]);
    input["tree"]["cards"][INTIMIDATE]["abilities"][0]["sources"][0] = source.clone();
    input["source_inventory"]["non_body"][0]["source_use"] = source.clone();
    input["source_inventory"]["non_body"][0]["declaration"]["keywords"] = json!(keywords);
    input["declaration_sidecar"][0]["source_use"] = source;
    input["declaration_sidecar"][0]["source_input_hash"] = json!(format!("sha256:{hash}"));
    input["declaration_sidecar"][0]["declaration"]["keywords"] = json!(keywords);
    let snapshot = snapshot()
        .lines()
        .map(|line| {
            let mut record: Value = serde_json::from_str(line).unwrap();
            if record["number"] == INTIMIDATE {
                record["faces"][0]["text"] = json!(text);
            }
            record.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (selection, snapshot)
}

#[test]
fn intimidate_ward_combination_does_not_create_an_impossible_attack_obligation() {
    // Official Q819; the combination is synthetic, all real-card tests retain their full text.
    for seat in ["P1", "P2"] {
        for (keywords, text, open) in [
            (vec!["ward"], "【守護】", false),
            (vec!["ward", "intimidate"], "【守護】【威圧】", true),
        ] {
            let (selection, snapshot) = synthetic_combination(&keywords, text);
            for (target, allowed) in [
                (format!("{}.leader", opponent(seat)), open),
                ("other".into(), open),
                ("ward".into(), !open),
            ] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] =
                    json!([{"id":"attacker","card":"plain"}]);
                initial["players"][opponent(seat)]["zones"]["field"] = json!([
                    {"id":"ward","card":INTIMIDATE,"state":{"acted":true}},
                    {"id":"other","card":"plain","state":{"acted":true}}
                ]);
                let mut game = selected_game(
                    &initial,
                    selection.clone(),
                    &snapshot,
                    &producer(vec![], None),
                );
                assert_eq!(
                    attack(&mut game, "attacker", &target),
                    if allowed { "resolved" } else { "cannot-attack" }
                );
            }
        }
    }
}

#[test]
fn the_complete_seven_keyword_handoff_is_closed_and_consumed_in_order() {
    let keywords = [
        "ward",
        "aura",
        "storm",
        "rush",
        "bane",
        "intimidate",
        "assail",
    ];
    let (selection, _) = synthetic_combination(
        &keywords,
        "【守護】【オーラ】【疾走】【突進】【必殺】【威圧】【指定攻撃】",
    );
    let input = &selection["authors"][0]["input"];
    let lowerer = lowering::ExpandedLowerer::new().unwrap();
    let candidate = lowerer.lower_current(input).unwrap();
    assert_eq!(candidate.program.card().abilities.len(), 7);
    for (ability, keyword) in candidate.program.card().abilities.iter().zip([
        KeywordId::Ward,
        KeywordId::Aura,
        KeywordId::Storm,
        KeywordId::Rush,
        KeywordId::Bane,
        KeywordId::Intimidate,
        KeywordId::Assail,
    ]) {
        assert!(
            matches!(ability.node.source.origin, AbilityOrigin::Keyword {keyword: found, ..} if found == keyword)
        );
    }
    let mut unsupported = input.clone();
    unsupported["source_inventory"]["non_body"][0]["declaration"]["keywords"]
        .as_array_mut()
        .unwrap()
        .push(json!("drain"));
    lowerer.lower_current(&unsupported).unwrap_err();
}
