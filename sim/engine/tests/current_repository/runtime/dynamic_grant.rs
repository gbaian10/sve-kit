#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the card scenarios."
)]
use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial
}

fn declaration(game: &mut Game, card: &str) -> CardPlay {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}

fn target(game: &mut Game, play: CardPlay, number: &str) -> CardPlay {
    let CardPlay::Awaiting(q) = play else {
        panic!("target question")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("targets")
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(number))
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![handle]))
        .unwrap()
}

fn attack(game: &mut Game, holder: &str, recipient: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":holder,"target":recipient}))
            .unwrap()
            .outcome,
        "resolved",
        "the recipient reaches a real attack window"
    );
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
}

fn heal(game: &mut Game, provider: &str, before: u8, damage: u8) -> Activation {
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        before,
        "Drain waits for its own resolution"
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Drain pending")
    };
    assert_eq!(
        options.len(),
        1,
        "CR 12.13.3 merges multiple Drain abilities"
    );
    assert_eq!(
        options[0].card, provider,
        "the grant keeps its text provider"
    );
    let activation = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        before.checked_add(damage).unwrap(),
        "Drain recovers the actual attack damage"
    );
    game.resume_internal_card().unwrap();
    activation
}

fn ability(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id
}

const CONTROLS: [(&str, &str, &str); 6] = [
    ("grant-absolute-only", "absolute-only", "フォロワー"),
    ("grant-demon-only", "demon-only", "フォロワー"),
    ("grant-one", "没頭の実験体", "フォロワー"),
    ("grant-many", "多頭の実験体", "フォロワー"),
    ("grant-aluzard", "永劫の吸血鬼・アルザード", "フォロワー"),
    (
        "grant-aluzard-form",
        "永劫の吸血鬼・アルザード",
        "フォロワー・エボルヴ",
    ),
];

fn controlled_game(initial: &Value) -> Game {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind) in CONTROLS {
            let traits: &[&str] = match number {
                "grant-absolute-only" => &["絶傑"],
                "grant-demon-only" => &["魔界"],
                _ => &[],
            };
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":traits,"cost":if kind.contains("エボルヴ") {"-"} else {"2"},"power":"2","hp":"3","text":"","sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::select(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "grant-controls",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

#[test]
fn bp15_085_evolution_grants_drain_only_to_the_selected_absolute_demon() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP15-084"}, {"id":"other","card":"BP01-037"}, {"id":"absolute-only","card":"grant-absolute-only"}, {"id":"demon-only","card":"grant-demon-only"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-085"}]);
    let mut game = controlled_game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let play = declaration(&mut game, "BP15-085");
    let CardPlay::Awaiting(q) = &play else {
        panic!("qualified target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("targets")
    };
    assert_eq!(domain.members.len(), 1, "both printed traits are required");
    resolved(target(&mut game, play, "BP15-084"));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["other"]["keywords"],
        json!([])
    );
    attack(&mut game, "source", "P2.leader");
    heal(&mut game, "BP15-085", 10, 2);
}

#[test]
fn bp16_089_creates_bats_and_grants_one_bat_rush_stats_and_drain() {
    for lethal in [false, true] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP16-089"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"power":if lethal {10_u8} else {0_u8},"hp":8_u8,"max_hp":8_u8}}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let fanfare = declaration(&mut game, "BP16-089");
        resolved(fanfare);
        game.resume_internal_card().unwrap();
        let created = game.projection(View::Referee).unwrap();
        let bats = created["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|id| *id != "source")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(bats.len(), 2);
        let play = game
            .activate_internal_card("source", ability("BP16-089", 2), PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(q) = play else {
            panic!("token target")
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("targets")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "ordinary Vampire provider is excluded"
        );
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        let bat = bats
            .iter()
            .find(|id| view["objects"][*id]["power"] == 2_u8)
            .unwrap();
        assert_eq!(view["objects"][bat]["hp"], 2_u8);
        assert_eq!(view["objects"]["source"]["acted"], true);
        assert_eq!(view["objects"]["source"]["keywords"], json!([]));
        attack(&mut game, bat, "enemy");
        if lethal {
            let after = game.projection(View::Referee).unwrap();
            assert!(
                !after["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(bat))
            );
            assert!(
                !after["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(bat))
            );
        }
        heal(&mut game, "BP16-089", 10, 2);
    }
}

#[test]
fn cp02_107_all_printings_choose_between_bane_and_drain_for_another_idol() {
    for printing in ["CP02-107", "CP02-P87"] {
        for drain in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"ally","card":"CP02-014"},{"id":"other","card":"BP01-037"}]);
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "grant-title",
            )
            .unwrap();
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let play = declaration(&mut game, printing);
            let CardPlay::Awaiting(target_question) = &play else {
                panic!("idol target")
            };
            let InputDomain::Targets(domain) = &target_question.domain else {
                panic!("targets")
            };
            assert_eq!(domain.members.len(), 1);
            let play = target(&mut game, play, "CP02-014");
            let CardPlay::Awaiting(q) = play else {
                panic!("keyword choice")
            };
            resolved(
                game.answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Modes(vec![ModeId(if drain { 2 } else { 1 })]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            attack(&mut game, "other", "P2.leader");
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "an ungranted follower has no Drain"
            );
            attack(&mut game, "ally", "P2.leader");
            if drain {
                heal(&mut game, printing, 10, 2);
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                    10_u8
                );
            }
        }
    }
}

#[test]
fn bp17_086_evolution_checks_crimson_before_installing_drain() {
    for crimson in [false, true] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-085"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-086"}]);
        let mut game = with_producer(
            &initial,
            Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(-1)),
                },
            },
        );
        if crimson {
            resolved(
                game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":4_u8}}),
        )
        .unwrap();
        resolved(declaration(&mut game, "BP17-086"));
        game.resume_internal_card().unwrap();
        attack(&mut game, "source", "P2.leader");
        if crimson {
            heal(&mut game, "BP17-086", 9, 4);
        } else {
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn bp19_043_all_printings_grant_drain_to_both_named_experiments() {
    for printing in ["BP19-043", "BP19-P12"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":printing}, {"id":"experiment","card":"grant-one"}, {"id":"many","card":"grant-many"}, {"id":"other","card":"BP01-037"}
        ]);
        let mut game = controlled_game(&initial);
        let play = game
            .activate_internal_card("source", ability(printing, 2), PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(q) = &play else {
            panic!("named experiment")
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("targets")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "both exact names are admitted; other followers are excluded"
        );
        resolved(target(&mut game, play, "grant-one"));
        game.resume_internal_card().unwrap();
        let damage = u8::try_from(
            game.projection(View::Referee).unwrap()["objects"]["experiment"]["power"]
                .as_u64()
                .unwrap(),
        )
        .unwrap();
        attack(&mut game, "experiment", "P2.leader");
        heal(&mut game, printing, 10, damage);
    }
}

#[test]
fn printed_drain_and_a_granted_copy_recover_only_once() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP02-030"}]);
    let mut game = with_producer(
        &initial,
        Action::Grant {
            subjects: subjects(),
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Drain,
            }],
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    attack(&mut game, "source", "P2.leader");
    heal(&mut game, "CP02-030", 10, 3);
}

#[test]
fn bp13_p31_all_printings_grant_drain_and_count_only_named_face_up_forms() {
    for printing in ["BP13-P31", "BP13-T03"] {
        for count in [2_u8, 3_u8] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"spell","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"ally","card":"grant-aluzard"}, {"id":"other","card":"BP01-037"}
            ]);
            let mut forms = (0..count).map(|index| json!({"id":format!("form-{index}"),"card":"grant-aluzard-form","face_up":true})).collect::<Vec<_>>();
            forms.push(json!({"id":"hidden","card":"grant-aluzard-form","face_up":false}));
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!(forms);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"power":0_u8,"hp":2_u8,"max_hp":2_u8}}]);
            let mut game = controlled_game(&initial);
            resolved(
                game.play_internal_card("spell", PlayerId { seat: 1 })
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let view = game.projection(View::Referee).unwrap();
            let bonus = if count == 3 { 3_u8 } else { 0_u8 };
            assert_eq!(view["P1"]["leader"]["life"], 9_u8);
            assert_eq!(view["objects"]["enemy"]["hp"], 1_u8);
            assert_eq!(view["objects"]["ally"]["power"], 3_u8 + bonus);
            assert_eq!(view["objects"]["ally"]["hp"], 3_u8 + bonus);
            assert_eq!(view["objects"]["other"]["power"], 5_u8);
            attack(&mut game, "ally", "P2.leader");
            heal(&mut game, printing, 9, 3 + bonus);
        }
    }
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Any unrelated question fails this fixed EX-payment scenario."
)]
fn bp19_056_all_printings_pay_two_ex_cards_for_storm_and_drain_and_cannot_replay_from_ex() {
    for printing in ["BP19-056", "BP19-SL14", "BP19-U04"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"cost-1","card":"BP19-064"}, {"id":"cost-2","card":"BP19-066"}
        ]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let mut play = declaration(&mut game, printing);
        while let CardPlay::Awaiting(q) = &play {
            let answer = match &q.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                    domain.members.iter().map(|member| member.handle).collect(),
                ),
                _ => panic!("two EX materials"),
            };
            play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        resolved(play);
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["cost-1", "cost-2"])
        );
        attack(&mut game, "source", "P2.leader");
        heal(&mut game, printing, 10, 6);
        resolved(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let lastword = declaration(&mut game, printing);
        resolved(lastword);
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["source"])
        );
        let error = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            matches!(&error, EngineFailure::Unsupported(reason) if reason == "internal D1 does not support card play holder, permission or timing"),
            "EX play must be rejected by its printed restriction: {error:?}"
        );
    }
}
