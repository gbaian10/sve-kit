#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the complete BP20 source and payment scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
struct Prepared {
    selected: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}
fn prepared() -> &'static Prepared {
    static READY: OnceLock<Prepared> = OnceLock::new();
    READY.get_or_init(||{
        let (mut snapshot,keywords,mut input)=producer_inputs();
        let mut programs=Vec::new();
        for (number,kind,cost,power,hp,traits) in [
            ("block-enemy","フォロワー","1","1","30",vec![]),
            ("bp20-mage","フォロワー","1","1","5",vec!["絶傑","魔法使い"]),
            ("bp20-costly","フォロワー","4","1","5",vec!["絶傑","魔法使い"]),
            ("bp20-absolute-follower","フォロワー","1","1","5",vec!["絶傑"]),
            ("bp20-mage-follower","フォロワー","1","1","5",vec!["魔法使い"]),
            ("bp20-fuel","スペル","1","-","-",vec!["絶傑","魔法使い"]),
            ("bp20-absolute-spell","スペル","1","-","-",vec!["絶傑"]),
            ("bp20-mage-spell","スペル","1","-","-",vec!["魔法使い"]),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"traits":traits,"cost":cost,"power":power,"hp":hp,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
            programs.push((number.to_owned(),WellFormedCardProgram::check(CardProgram{abilities:vec![],card_play_costs:vec![]}).unwrap()));
        }
        Prepared{selected:repository::selection_with_rules_from(&snapshot,&keywords,&input),programs}
    })
}
fn load(game: &mut Game) {
    for (number, program) in &prepared().programs {
        game.load_internal_card(number, program).unwrap();
    }
}
fn build(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "block-migrations",
    )
    .unwrap();
    load(&mut game);
    game
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"block-enemy","owner":"P1"}]);
    initial
}
fn finish(game: &mut Game, mut play: CardPlay, modes: &[u64]) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let n = domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0
                    .min(u64::try_from(domain.members.len()).unwrap());
                let handles = domain
                    .members
                    .iter()
                    .take(usize::try_from(n).unwrap())
                    .map(|member| member.handle)
                    .collect();
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|member| member.handle).collect())
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|member| member.handle)
                    .collect(),
            )),
            _ => panic!("unsupported migration question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn settle(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending ability");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, modes);
    }
}
fn enter(initial: &Value, zone: &str) -> Game {
    if zone == "hand" || zone == "ex" {
        let mut game = build(initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[1]);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        game
    } else {
        let action = Action::Move {
            objects: subjects_in(ZoneKind::Cemetery),
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
        };
        let mut game = with_selected_producer(initial, action, &prepared().selected);
        load(&mut game);
        let play = game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[1]);
        game.resume_internal_card().unwrap();
        game
    }
}
#[test]
fn bp20_052_discards_once_and_only_nonhand_entry_adds_two_health() {
    for zone in ["hand", "ex"] {
        let mut game = enter(&position("BP20-052", zone), zone);
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1);
        assert_eq!(
            state["objects"]["source"]["hp"],
            if zone == "hand" { 3_u8 } else { 5 },
            "only nonhand entry adds two health"
        );
    }
}
#[test]
fn bp20_053_cycles_a_card_and_heals_only_after_nonhand_entry() {
    for zone in ["hand", "ex"] {
        let mut game = enter(&position("BP20-053", zone), zone);
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if zone == "hand" { 10_u8 } else { 12 },
            "only nonhand entry heals the controller"
        );
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1);
    }
}

fn cemetery(initial: &mut Value, two: bool) {
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"wrong-absolute","card":"bp20-absolute-follower"},
        {"id":"wrong-mage","card":"bp20-mage-follower"},
        {"id":"costly","card":"bp20-costly"},
        {"id":"revive","card":"bp20-mage"}
    ]);
    if two {
        initial["players"]["P1"]["zones"]["cemetery"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"revive2","card":"bp20-mage"}));
    }
}
fn token_id(game: &Game) -> String {
    game.projection(View::Referee).unwrap()["P1"]["ex"][0]
        .as_str()
        .unwrap()
        .to_owned()
}
#[test]
fn bp20_042_hand_activation_discards_itself_and_only_a_spell_with_both_printed_traits() {
    let mut initial = position("BP20-042", "hand");
    cemetery(&mut initial, false);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"fuel","card":"bp20-fuel"}),
            json!({"id":"only-absolute","card":"bp20-absolute-spell"}),
            json!({"id":"only-mage","card":"bp20-mage-spell"}),
            json!({"id":"wrong-kind","card":"bp20-mage"}),
        ]);
    let mut game = build(&initial);
    let definition = prepared().selected.current["BP20-042"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated && ability.source.line == 3)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(materials) = game
        .activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private hand cost");
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("chosen spell material");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "neither one trait alone nor a follower can pay"
    );
    assert_eq!(domain.members[0].card.as_deref(), Some("bp20-fuel"));
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot see private material candidates"
    );
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![])
        )
        .is_err(),
        "the complete ordered cost must be paid"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "an incomplete hand payment must preserve the whole position"
    );
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    settle(&mut game, &[1]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["hand"],
        json!(["only-absolute", "only-mage", "wrong-kind"]),
        "only the source and the spell with both traits are discarded"
    );
    assert_eq!(after["P1"]["ex"].as_array().unwrap().len(), 1);
    assert_eq!(after["P1"]["pp"]["current"], 10_u8);
    let token = token_id(&game);
    let play = game
        .play_internal_card(&token, PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, &[1]));
    settle(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["revive"]));
    assert_eq!(
        state["P1"]["pp"]["current"], 9_u8,
        "a token created by the hand activation has its ordinary cost"
    );
}
#[test]
fn bp20_042_nonhand_entry_evolves_into_real_043_and_both_created_spells_are_discounted() {
    for zone in ["hand", "ex"] {
        let mut initial = position("BP20-042", zone);
        cemetery(&mut initial, true);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP20-043"}]);
        let mut game = enter(&initial, zone);
        let entry_pp = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone();
        settle(&mut game, &[1]);
        let entered = game.projection(View::Referee).unwrap();
        assert_eq!(entered["objects"]["source"]["evolved"], zone == "ex");
        assert_eq!(
            entered["P1"]["pp"]["current"], entry_pp,
            "effect evolution never pays the printed activation cost"
        );
        if zone == "hand" {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved",
                "ordinary evolution remains available after hand entry"
            );
            settle(&mut game, &[1]);
        }
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["objects"]["enemy"]["damage"], 9_u8);
        assert_eq!(evolved["P1"]["ex"].as_array().unwrap().len(), 2);
        let pp = evolved["P1"]["pp"]["current"].clone();
        for expected in ["revive", "revive2"] {
            let token = token_id(&game);
            let play = game
                .play_internal_card(&token, PlayerId { seat: 1 })
                .unwrap();
            resolved(finish(&mut game, play, &[1]));
            settle(&mut game, &[1]);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"], pp,
                "both receipt-bound created spells carry the turn discount"
            );
            assert!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(expected)),
                "the spell returns only the cheap follower with both traits"
            );
        }
    }
}
