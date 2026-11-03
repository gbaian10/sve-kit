use super::union_burst_batch1::{finish, pending};
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

struct Fixture {
    selection: SelectedEffects,
    base: WellFormedCardProgram,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let base_name = "nested-merlin-base";
        if !snapshot.ends_with('\n') {
            snapshot.push('\n');
        }
        snapshot.push_str(&json!({"number":base_name,"faces":[{"name":"マーリン","card_type":"フォロワー","card_class":"ウィッチ","cost":"3","power":"3","hp":"2","traits":["魔法使い"],"text":null,"sections":[]}]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["nested-base.yaml",json!({"format":1_u8,"kind":"effect_set","cards":{base_name:{"status":"complete","abilities":[]}}}).to_string()]));
        let mut tree = expanded::tree().clone();
        let mut base_template = tree["cards"]["synthetic-evolution"].clone();
        base_template["abilities"][0]["evolve"]["cost"]["pp"] = json!(2_u8);
        tree["cards"] = json!({base_name:base_template});
        let base_program = lowerer().lower(&tree).unwrap().remove(base_name).unwrap();
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml")).unwrap()).unwrap();
        rules["input"]["snapshot_sha256"] = json!(rule_fixtures::hash(snapshot.as_bytes()));
        let selection = repository::select_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap();
        Fixture { selection, base: base_program }
    })
}
fn build(initial: &Value) -> Game {
    let prepared = fixture();
    let mut game = Game::from_selected_effects(
        prepared.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "nested-real-cards",
    )
    .unwrap();
    game.load_internal_card("nested-merlin-base", &prepared.base)
        .unwrap();
    game
}
fn aliases(representative: &str) -> Vec<String> {
    let selected = &fixture().selection;
    let identity = &selected.current[representative].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(number, _)| number.clone())
        .collect()
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
    initial
}

#[test]
fn octrice_all_printings_play_an_opponents_spell_for_zero_and_return_it_to_its_owner() {
    for card in aliases("BP05-018") {
        let mut initial = position(&card);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"borrowed","card":"SD03-005"}]);
        let mut game = build(&initial);
        let definition = fixture().selection.current[&card]
            .program
            .card()
            .abilities
            .iter()
            .find(|entry| entry.source.line == 4)
            .unwrap()
            .node
            .id;
        let play = game
            .activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap();
        let result = resolved(finish(&mut game, play));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["P2"]["cemetery"], json!(["borrowed"]));
        assert_eq!(state["P1"]["pp"]["current"], 2_u8);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(
                    fact,
                    Fact::CardPlayed {
                        controller: PlayerId { seat: 1 },
                        ..
                    }
                ))
                .count(),
            1
        );
    }
}
#[test]
fn galmieux_all_printings_replay_a_two_cost_omen_and_defer_its_fanfare_until_after_evolution() {
    for card in aliases("BP05-053") {
        let mut initial = position("BP05-052");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"replayed","card":"BP05-018"}]);
        initial["players"]["P2"]["zones"]["deck"] =
            json!([{"id":"milled0","card":"BP01-037"},{"id":"milled1","card":"BP01-037"}]);
        let mut game = build(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
        )
        .unwrap();
        let play = pending(&mut game, &card);
        let result = resolved(finish(&mut game, play));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "replayed"]));
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        assert_eq!(state["P2"]["cemetery"], json!([]));
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::CardResolved { .. }))
        );
        let fanfare = pending(&mut game, "BP05-018");
        resolved(finish(&mut game, fanfare));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["milled0", "milled1"])
        );
    }
}
#[test]
fn merlin_all_evolved_printings_exclude_the_played_spell_from_spellchain() {
    for card in aliases("BP01-053") {
        for count in [10_u8, 11_u8] {
            let mut initial = position("nested-merlin-base");
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            let mut cemetery = vec![
                json!({"id":"too_costly","card":"BP01-064"}),
                json!({"id":"replayed","card":"BP01-065"}),
            ];
            cemetery.extend(
                (0..count.saturating_sub(2))
                    .map(|i| json!({"id":format!("filler{i}"),"card":"SD03-005"})),
            );
            initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
            let mut game = build(&initial);
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
            )
            .unwrap();
            let CardPlay::Awaiting(targets) = pending(&mut game, &card) else {
                panic!("expected Merlin's cemetery target question");
            };
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("expected formal cemetery targets");
            };
            assert_eq!(domain.members.len(), usize::from(count.saturating_sub(1)));
            let selected = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some("BP01-065"))
                .unwrap()
                .handle;
            let play = game
                .answer_internal_card(
                    targets.actor,
                    targets.token,
                    Answer::Targets(vec![selected]),
                )
                .unwrap();
            let result = resolved(finish(&mut game, play));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["drawn", "spare"]));
            assert_eq!(
                state["P1"]["cemetery"].as_array().unwrap().len(),
                usize::from(count)
            );
            assert_eq!(
                state["P1"]["cemetery"].as_array().unwrap().last().unwrap(),
                "replayed"
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                if count == 11 { 9_u8 } else { 8_u8 }
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::CardPlayed { .. }))
                    .count(),
                1
            );
        }
    }
}
