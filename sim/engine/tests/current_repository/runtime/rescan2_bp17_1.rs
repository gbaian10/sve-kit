#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn finish(game: &mut Game, mut step: CardPlay, mode: u64, accept: bool) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let handles = domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect();
                if matches!(q.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement => panic!("unexpected card decision"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn play(game: &mut Game, mode: u64, accept: bool) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step, mode, accept);
}

fn trigger(game: &mut Game, mode: u64, accept: bool) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending ability")
    };
    assert_eq!(options.len(), 1, "only the intended ability is pending");
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step, mode, accept);
    game.resume_internal_card().unwrap();
}

// Unmigrated bases supply only an external evolution producer for the real form tests.
fn companions() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static FIXTURE: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        let mut cards = serde_json::Map::new();
        let mut programs = serde_json::Map::new();
        for form in ["BP17-024", "BP17-028", "BP17-032", "BP17-P08", "BP17-046"] {
            let printed = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|card| card["number"] == form)
                .unwrap();
            let number = format!("evolution-{form}");
            snapshot.push_str(
                &json!({"number":number,"faces":[{
                    "name":printed["faces"][0]["name"],"card_class":"ニュートラル",
                    "card_type":"フォロワー","cost":"1","power":"1","hp":"1",
                    "traits":[],"text":null,"sections":[]
                }]})
                .to_string(),
            );
            snapshot.push('\n');
            cards.insert(number.clone(), json!({"status":"complete","abilities":[]}));
            let mut program = template.clone();
            program["abilities"][0]["evolve"]["cost"]["pp"] =
                json!(if form == "BP17-028" { 3_u8 } else { 1_u8 });
            programs.insert(number, program);
        }
        input["documents"].as_array_mut().unwrap().push(json!([
            "rescan2-bp17-evolution.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()
        ]));
        tree["cards"] = Value::Object(programs);
        (
            Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap(),
            lowerer().lower(&tree).unwrap(),
        )
    })
}

fn companion_game(initial: &Value) -> Game {
    let (selected, programs) = companions();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "rescan2-bp17",
    )
    .unwrap();
    for (card, program) in programs {
        game.load_internal_card(card, program).unwrap();
    }
    game
}

fn evolution(base: &str, form: &str, cost: u8) -> Game {
    let synthetic = format!("evolution-{form}");
    let base = if companions().1.contains_key(&synthetic) {
        synthetic.as_str()
    } else {
        base
    };
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = companion_game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "normal evolution pays the selected printed cost"
    );
    game
}

#[test]
fn bp17_010_evolution_deals_two_for_both_printings() {
    for form in ["BP17-010", "BP17-P03"] {
        let mut game = evolution("BP17-009", form, 1);
        trigger(&mut game, 1, true);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(s["P1"]["pp"]["current"], 9_u8);
        assert_eq!(s["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn bp17_028_evolution_deals_five() {
    let mut game = evolution("BP17-027", "BP17-028", 3);
    trigger(&mut game, 1, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp17_032_evolution_deals_two_for_both_printings() {
    for form in ["BP17-032", "BP17-P08"] {
        let mut game = evolution("BP17-031", form, 1);
        trigger(&mut game, 1, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
    }
}

#[test]
fn bp17_046_evolution_deals_five() {
    let mut game = evolution("BP17-045", "BP17-046", 1);
    trigger(&mut game, 1, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp17_022_entry_from_ex_replaces_one_damage_with_three() {
    for card in ["BP17-022", "BP17-SL08"] {
        for zone in ["hand", "ex"] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
            let mut game = game(&initial);
            play(&mut game, 1, true);
            trigger(&mut game, 1, true);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["objects"]["enemy"]["damage"],
                if zone == "hand" { 1_u8 } else { 3_u8 }
            );
            assert_eq!(s["P2"]["leader"]["life"], 20_u8);
        }
    }
}

#[test]
fn bp17_034_attack_damages_the_selected_follower_and_its_leader() {
    let mut initial = position("BP17-034");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-034"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game, 1, true);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(s["P2"]["leader"]["life"], 17_u8);
    assert_eq!(s["P1"]["leader"]["life"], 20_u8);
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(&mut game, step, 1, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        12_u8
    );
}

#[test]
fn bp17_037_fanfare_discards_before_the_chosen_damage_or_draw() {
    for card in ["BP17-037", "BP17-SL09", "PR-461"] {
        for mode in [1_u64, 2] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"payer","card":"BP01-171"}));
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
            let mut game = game(&initial);
            play(&mut game, mode, true);
            trigger(&mut game, mode, true);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(s["P1"]["cemetery"], json!(["payer"]));
            assert_eq!(
                s["objects"]["enemy"]["damage"],
                if mode == 1 { 3_u8 } else { 0_u8 }
            );
            assert_eq!(
                s["P1"]["hand"],
                if mode == 1 {
                    json!([])
                } else {
                    json!(["drawn"])
                }
            );
        }
    }
    let mut initial = position("BP17-037");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"payer","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game, 1, true);
    trigger(&mut game, 1, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["payer"])
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        0_u8
    );
}

#[test]
fn bp17_024_evolution_destroys_up_to_two_enemy_followers() {
    let mut initial = position("BP17-023");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"evolution-BP17-024"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-024"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second","card":"BP01-037"}));
    let mut game = companion_game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game, 1, true);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P2"]["field"], json!([]));
    assert_eq!(s["P2"]["cemetery"], json!(["enemy", "second"]));
    assert_eq!(s["P1"]["field"], json!(["source"]));
}

#[test]
fn bp17_110_all_printings_pay_one_pp_for_normal_evolution() {
    for card in ["BP17-110", "BP17-SL26", "PR-586"] {
        let game = evolution(card, "BP17-111", 1);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["P1"]["pp"]["current"], 9_u8);
        assert_eq!(s["objects"]["source"]["power"], 3_u8);
        assert_eq!(s["objects"]["source"]["hp"], 3_u8);
    }
}
