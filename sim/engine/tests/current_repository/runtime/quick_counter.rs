//! Printed payment fragments do not admit Pamela, Oz or the external test producer as complete cards.
#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the payment scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, QuickOption};

struct Fixture {
    selection: SelectedEffects,
    programs: BTreeMap<String, WellFormedCardProgram>,
    spell_price: i64,
}

fn fragment(number: &str, headers: &[(u32, Value)], cost: u64) -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let template = seed["abilities"][0].clone();
    let abilities = headers
        .iter()
        .map(|(line, header)| {
            let mut entry = template.clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry["line"] = json!(line);
            entry
                .as_object_mut()
                .unwrap()
                .extend(header.as_object().unwrap().clone());
            if header.get("keywords").is_some() {
                entry["sources"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("ability_scope");
            }
            entry
        })
        .collect::<Vec<_>>();
    seed["abilities"] = json!(abilities);
    tree["cards"] = json!({number:seed});
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([(number.to_owned(), Nat(cost))]))
        .unwrap()
        .remove(number)
        .unwrap()
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (snapshot, keywords, original) = inputs();
        let mut input = original.clone();
        input["authors"].as_array_mut().unwrap().retain(|author| {
            author["input"]["card_no"] == "BP01-075"
        });
        assert_eq!(input["authors"].as_array().unwrap().len(), 1, "the existing Stack declaration supplies the current proof");
        let cards = ["BP02-037", "BP03-038", "BP01-065", "BP01-097"]
            .into_iter()
            .map(|number| (number.to_owned(), json!({
                "status":"complete", "review":"payment fixture only", "abilities":[]
            })))
            .collect::<serde_json::Map<_, _>>();
        input["documents"] = json!([["quick-payment-fixture.yaml", json!({
            "format":1_u8, "kind":"effect_set", "cards":cards
        }).to_string()]]);
        let selection = repository::select_without_rules(snapshot, keywords, &input).unwrap();
        let pamela = [
            (2, json!({"activate":{
                "quick":true,
                "cost":{"steps":[{"act_self":true},{"earth_rite":1_u8}]},
                "targets":[{"bind":"chosen","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_u8}],
                "do":[{"buff":{"on":"chosen","power":-2_i8,"hp":-2_i8}}]
            }})),
            (3, json!({"activate":{
                "quick":true,
                "cost":{"steps":[{"act_self":true},{"earth_rite":1_u8}]},
                "targets":[{"bind":"chosen","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1_u8}],
                "do":[{"buff":{"on":"chosen","power":2_u8,"hp":2_u8}}]
            }})),
        ];
        let oz = [(2, json!({"activate":{
            "quick":true, "per_turn":1_u8, "cost":{"earth_rite":1_u8},
            "do":[{"player_effect":{"who":"self","until":"turn","effect":{
                "cost_mod":{"when_playing":{"who":"self","kind":"spell"},"delta":-4_i8,"next_only":true}
            }}}]
        }}))];
        let mut programs = BTreeMap::new();
        programs.insert("BP02-037".into(), fragment("BP02-037", &pamela, 4));
        programs.insert("BP03-038".into(), fragment("BP03-038", &oz, 6));
        let spell_price = snapshot.lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["number"] == "BP01-065").unwrap()["faces"][0]["cost"]
            .as_str().unwrap().parse::<i64>().unwrap();
        let spell_cost = u64::try_from(spell_price).unwrap();
        programs.insert("BP01-065".into(), fragment(
            "BP01-065", &[(1, json!({"spell":{"do":[]}}))], spell_cost,
        ));
        programs.insert("BP01-097".into(), fragment("BP01-097", &[(1, json!({"activate":{
            "cost":{"pp":0_u8},
            "targets":[{"bind":"selected","from":{"who":"self","zone":"cemetery","kind":"spell"},"n":1_u8}],
            "do":[{"play_card":{"ref":"selected","cost":0_u8}}]
        }}))], 2));
        Fixture { selection, programs, spell_price }
    })
}

fn board(initial: &Value) -> Game {
    let fixture = fixture();
    let mut game = Game::from_selected_effects(
        fixture.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-quick-payment",
    )
    .unwrap();
    for (number, program) in &fixture.programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}

fn definition(number: &str, line: u32) -> AbilityId {
    fixture().programs[number]
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("payment question")
    };
    q
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected payment stage"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "all declared payment stages resolve"
    );
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn quick(game: &mut Game) -> (DecisionView, Vec<QuickOption>) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick")
    };
    let options = options.clone();
    (q, options)
}

#[test]
fn pamela_printed_payment_and_other_follower_target_match_main_attack_and_end_quick() {
    for entry in ["main", "attack", "end"] {
        for line in [2, 3] {
            let mut initial = setup(if entry == "main" { "P1" } else { "P2" }, "P1", [20, 20]);
            initial["players"]["P1"]["zones"] = json!({"field":[
                {"id":"source","card":"BP02-037"},
                {"id":"ally","card":"BP02-037","state":{"silenced":true}},
                {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}
            ],"hand":[{"id":"hand-copy","card":"BP02-037"}]});
            initial["players"]["P2"]["zones"] = json!({"field":[{"id":"enemy","card":"BP02-037","state":{"entered_this_turn":false,"silenced":true}}]});
            let mut game = board(&initial);
            let before = game.projection(View::Referee).unwrap();
            let declaration = if entry == "main" {
                game.activate_internal_card(
                    "source",
                    definition("BP02-037", line),
                    PlayerId { seat: 1 },
                )
                .unwrap()
            } else {
                let action = if entry == "attack" {
                    json!({"do":"attack","attacker":"enemy","target":"P1.leader"})
                } else {
                    json!({"do":"end-phase"})
                };
                game.internal_rule_action(&action).unwrap();
                let (q, options) = quick(&mut game);
                assert_eq!(
                    options.len(),
                    2,
                    "only the two field abilities have Quick permission"
                );
                assert!(options.iter().all(|option| option.zone == "field"));
                let quoted = game.projection(View::Referee).unwrap();
                assert_eq!(quoted["objects"]["source"]["acted"], false);
                assert_eq!(
                    quoted["objects"]["soil"]["counters"]["スタックカウンター"],
                    1_u8
                );
                let handle = options
                    .iter()
                    .find(|option| option.line == Some(u64::from(line)))
                    .unwrap()
                    .handle;
                game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(handle)))
                    .unwrap()
            };
            let target = awaiting(declaration);
            let InputDomain::Targets(targets) = &target.domain else {
                panic!("printed target")
            };
            assert_eq!(
                targets.members.len(),
                1,
                "the friendly ability excludes its own source"
            );
            let materials = awaiting(
                game.answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![targets.members[0].handle]),
                )
                .unwrap(),
            );
            let InputDomain::CostMaterials(pool) = &materials.domain else {
                panic!("Earth Rite")
            };
            assert_eq!(
                pool.members.len(),
                1,
                "only the own field Stack amulet pays"
            );
            let paid = game
                .answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![pool.members[0].handle]),
                )
                .unwrap();
            assert!(matches!(paid, CardPlay::Resolved(_)));
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["source"]["acted"], true);
            assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
            assert_eq!(after["P1"]["cemetery"], json!(["soil"]));
            let affected = if line == 2 { "enemy" } else { "ally" };
            let delta = if line == 2 { -2 } else { 2 };
            assert_eq!(
                after["objects"][affected]["power"].as_i64().unwrap(),
                before["objects"][affected]["power"].as_i64().unwrap() + delta
            );
            assert_eq!(
                after["objects"][affected]["hp"].as_i64().unwrap(),
                before["objects"][affected]["hp"].as_i64().unwrap() + delta
            );
        }
    }
}

fn oz_position() -> Value {
    let mut initial = setup("P1", "P1", [20, 20]);
    initial["players"]["P1"]["zones"] = json!({"field":[
        {"id":"oz-one","card":"BP03-038"},{"id":"oz-two","card":"BP03-038"},
        {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":8_u8}}},
        {"id":"effect-source","card":"BP01-097"}
    ],"cemetery":[{"id":"effect-spell","card":"BP01-065"}],"hand":[{"id":"held-spell","card":"BP01-065"},{"id":"later-spell","card":"BP01-065"}],
    "deck":[{"id":"p1draw","card":"BP01-097"}]});
    initial["players"]["P2"]["zones"] = json!({"deck":[{"id":"p2draw","card":"BP01-097"}]});
    initial
}

fn use_oz(game: &mut Game, id: &str) {
    let play = game
        .activate_internal_card(id, definition("BP03-038", 2), PlayerId { seat: 1 })
        .unwrap();
    finish(game, play);
    settle(game);
}

fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if let Some(q) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
}

#[test]
fn oz_printed_quota_is_per_source_and_both_turns_effect_play_consumes_next_spell() {
    let mut game = board(&oz_position());
    use_oz(&mut game, "oz-one");
    let before = format!("{game:?}");
    assert!(matches!(
        game.activate_internal_card("oz-one", definition("BP03-038", 2), PlayerId { seat: 1 }),
        Err(EngineFailure::IllegalPlay(_))
    ));
    assert_eq!(format!("{game:?}"), before);
    use_oz(&mut game, "oz-two");
    let target = awaiting(
        game.activate_internal_card(
            "effect-source",
            definition("BP01-097", 1),
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("effect Play target")
    };
    assert_eq!(domain.members.len(), 1);
    let played = game
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap();
    finish(&mut game, played);
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8
    );
    let held_play = game
        .play_internal_card("held-spell", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, held_play);
    settle(&mut game);
    let price = fixture().spell_price;
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10 - price,
        "the effect-played Spell consumed both independently installed discounts"
    );
    end_turn(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let (q, options) = quick(&mut game);
    assert_eq!(
        options.len(),
        2,
        "both source quotas reset on the opponent's actual turn"
    );
    assert!(
        options
            .iter()
            .all(|option| option.zone == "field" && option.line == Some(2))
    );
    let enemy_play = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
        .unwrap();
    finish(&mut game, enemy_play);
    settle(&mut game);
    let (followup, remaining) = quick(&mut game);
    assert_eq!(
        remaining.len(),
        1,
        "the other source still has its independent quota"
    );
    game.answer_internal_card(followup.actor, followup.token, Answer::Quick(None))
        .unwrap();
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    let pp_before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_i64()
        .unwrap();
    let later_play = game
        .play_internal_card("later-spell", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, later_play);
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        pp_before - price,
        "the unused opponent-turn discount expired at that actual turn's end"
    );
    use_oz(&mut game, "oz-one");
    use_oz(&mut game, "oz-two");
}
