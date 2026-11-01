#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate printed cost scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

struct Fixture {
    selection: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}
fn add(snapshot: &mut String, input: &mut Value, id: &str, face: &Value) {
    snapshot.push_str(&json!({"number":id,"faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([
        format!("{id}.yaml"),
        json!({"format":1_i64,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}})
            .to_string()
    ]));
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot,keywords,mut input)=inputs().clone();
        let rows=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
        let mut programs=vec![];
        // Explicit synthetic forms test source-card payment independently of unadmitted real forms.
        for number in ["BP05-048","BP05-052","BP07-086","BP13-078","BP04-106"] {
            let source=rows.iter().find(|row|row["number"]==number).unwrap();
            let id=format!("payment-form-{number}");
            add(&mut snapshot,&mut input,&id,&json!({"name":source["faces"][0_usize]["name"],"card_type":"フォロワー・エボルヴ","card_class":"ニュートラル","cost":"-","power":"10","hp":"10","traits":[],"text":"","sections":[]}));
            programs.push((id,WellFormedCardProgram::check(CardProgram{abilities:vec![],card_play_costs:vec![]}).unwrap()));
        }
        for (id,body) in [
            ("payment-hit",json!({"targets":[{"bind":"victim","from":{"who":"all","zone":"field","kind":"follower"},"n":1_i64}],"do":[{"damage":{"n":1_i64,"to":"victim"}}]})),
            ("payment-break",json!({"targets":[{"bind":"victim","from":{"who":"self","zone":"field","kind":"amulet"},"n":1_i64}],"do":[{"destroy":{"ref":"victim"}}]})),
        ] {
            add(&mut snapshot,&mut input,id,&json!({"name":id,"card_type":"スペル","card_class":"ニュートラル","cost":"0","power":"-","hp":"-","traits":[],"text":null,"sections":[]}));
            let mut tree=expanded::spell_tree().clone();let mut card=tree["cards"]["SD03-005"].clone();card["abilities"][0]["spell"]=body;
            tree["cards"]=json!({id:card});
            let program=lowerer().lower_with_printed_costs(&tree,&BTreeMap::from([(id.to_owned(),Nat(0))])).unwrap().remove(id).unwrap();
            programs.push((id.into(),program));
        }
        Fixture{selection:repository::selection_with_rules_from(&snapshot,&keywords,&input),programs}
    })
}
fn game(initial: &Value) -> Game {
    let prepared = fixture();
    let mut game = Game::from_selected_effects(
        prepared.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "payment-migration",
    )
    .unwrap();
    for (id, program) in &prepared.programs {
        game.load_internal_card(id, program).unwrap();
    }
    game
}
fn position(card: &str, zone: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_i64);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial
}
fn resolve(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) =
            &question.domain
        {
            let handles = domain
                .members
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
                .collect();
            if matches!(question.domain, InputDomain::Targets(_)) {
                Answer::Targets(handles)
            } else {
                Answer::CostMaterials(handles)
            }
        } else if matches!(question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(mode)])
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if let InputDomain::TokenCapacity { choices } = &question.domain {
            assert_eq!(choices.len(), 1_usize, "resolve");
            Answer::TokenCapacity(0)
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
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
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else {
            panic!("unexpected payment migration input");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn triggers(game: &mut Game, mode: u64) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending printed trigger");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolve(game, play, mode);
    }
}
fn play(game: &mut Game, id: &str, seat: u32, mode: u64) {
    let result = game.play_internal_card(id, PlayerId { seat }).unwrap();
    resolve(game, result, mode);
    triggers(game, mode);
}
fn evolve(game: &mut Game, pp: u8, costs: &Value) {
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp},"costs":costs})).unwrap().outcome,"resolved", "evolve");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before.checked_sub(u64::from(pp)).unwrap(),
        "evolve"
    );
    triggers(game, 1);
}
#[test]
fn bp05_048_three_idols_set_only_the_sources_real_evolution_payment_to_zero() {
    for count in [1_u8, 2] {
        let mut initial = position("BP05-048", "hand", "payment-form-BP05-048");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("idol{i}"),"card":"BP05-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source", 1, 1);
        evolve(&mut game, if count == 2 { 0 } else { 2 }, &json!({}));
    }
}
#[test]
fn bp05_052_requires_ability_damage_during_its_own_turn_before_free_evolution() {
    for own_turn in [false, true] {
        let mut initial = position("BP05-052", "field", "payment-form-BP05-052");
        let seat = if own_turn { 1 } else { 2 };
        let actor = if own_turn { "P1" } else { "P2" };
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["hand"] = json!([{"id":"hit","card":"payment-hit"}]);
        let mut game = game(&initial);
        let result = game.play_internal_card("hit", PlayerId { seat }).unwrap();
        resolve(&mut game, result, 1);
        assert_eq!(
            game.internal_pending_choice().unwrap().is_some(),
            own_turn,
            "bp05 052 requires ability damage during its own turn before free evolution"
        );
        if own_turn {
            triggers(&mut game, 1);
            evolve(&mut game, 0, &json!({}));
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["damage"],
            1_i64,
            "bp05 052 requires ability damage during its own turn before free evolution"
        );
    }
}
#[test]
fn bp07_086_banishes_the_created_repair_mode_and_reduces_evolution_by_one() {
    let mut game = game(&position("BP07-086", "hand", "payment-form-BP07-086"));
    play(&mut game, "source", 1, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .len(),
        1_usize,
        "bp07 086 banishes the created repair mode and reduces evolution by one"
    );
    let definition = fixture().selection.current["BP07-086"]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| {
            entry.node.kind == AbilityKind::Activated
                && matches!(entry.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id;
    let result = game
        .activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap();
    resolve(&mut game, result, 1);
    triggers(&mut game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!([]),
        "bp07 086 banishes the created repair mode and reduces evolution by one"
    );
    evolve(&mut game, 6, &json!({}));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        1_i64,
        "bp07 086 banishes the created repair mode and reduces evolution by one"
    );
}
#[test]
fn bp13_078_puts_an_opponents_other_follower_into_its_owners_cemetery_then_reduces_evolution() {
    let mut initial = position("BP13-078", "hand", "payment-form-BP13-078");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"fee","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source", 1, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["fee"]),
        "bp13 078 puts an opponents other follower into its owners cemetery then reduces evolution"
    );
    evolve(&mut game, 1, &json!({}));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_i64,
        "bp13 078 puts an opponents other follower into its owners cemetery then reduces evolution"
    );
}
#[test]
fn bp04_106_acts_two_amulets_for_fanfare_then_banishes_one_to_evolve() {
    let mut initial = position("BP04-106", "hand", "payment-form-BP04-106");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"fee0","card":"BP07-T03"},{"id":"fee1","card":"BP07-T03"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, "source", 1, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["fee0"]["acted"], true,
        "bp04 106 acts two amulets for fanfare then banishes one to evolve"
    );
    assert_eq!(
        state["objects"]["fee1"]["acted"], true,
        "bp04 106 acts two amulets for fanfare then banishes one to evolve"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], 3_i64,
        "bp04 106 acts two amulets for fanfare then banishes one to evolve"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 23_i64,
        "bp04 106 acts two amulets for fanfare then banishes one to evolve"
    );
    evolve(&mut game, 0, &json!({"materials":["fee0"]}));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["fee1", "source"]),
        "evolution consumes exactly the selected amulet"
    );
}
#[test]
fn bp06_103_end_phase_requires_two_remaining_pp() {
    for pp in [1_u8, 2] {
        let mut initial = position("BP06-103", "field", "BP06-104");
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_i64});
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        triggers(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if pp == 2 { 19_i64 } else { 20_i64 },
            "bp06 103 end phase requires two remaining pp"
        );
    }
}
#[test]
fn bp06_104_real_discard_evolution_restores_two_pp() {
    let mut initial = position("BP06-103", "field", "BP06-104");
    initial["players"]["P1"]["pp"] = json!({"current":1_i64,"max":10_i64});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fee","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, 0, &json!({"discard":["fee"]}));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 3_i64,
        "bp06 104 real discard evolution restores two pp"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["fee"]),
        "bp06 104 real discard evolution restores two pp"
    );
}
