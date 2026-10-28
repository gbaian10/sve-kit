use super::*;
use sve_engine::catalog::EngineIdentityInput;

#[expect(
    dead_code,
    reason = "Only the evolution branch of the shared expansion fixture is used."
)]
#[path = "../../support/expanded.rs"]
mod expanded;
#[path = "../../support/rule_fixtures.rs"]
mod rule_fixtures;

fn evolution_companion(snapshot: &str, input: &mut Value) -> (String, WellFormedCardProgram) {
    let form: Value = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|record| record["number"] == "BP01-156")
        .unwrap();
    // A synthetic base supplies the evolution producer; the real evolved program stays intact.
    let companion = json!({"number":"synthetic-evolution","faces":[{
        "name":form["faces"][0]["name"],"card_class":"ニュートラル","card_type":"フォロワー",
        "cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]
    }]});
    let snapshot = format!("{}\n{companion}", snapshot.trim_end());
    input["documents"].as_array_mut().unwrap().push(json!(["ex-companion.yaml",json!({
        "format":1_u8,"kind":"effect_set","cards":{"synthetic-evolution":{"status":"complete","abilities":[]}}
    }).to_string()]));
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"synthetic-evolution":tree["cards"]["synthetic-evolution"]});
    let mut programs = ExpandedLowerer::new().unwrap().lower(&tree).unwrap();
    (snapshot, programs.remove("synthetic-evolution").unwrap())
}

fn ex_position(available: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"base","card":"synthetic-evolution"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-156"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"own-ex","card":"BP01-168"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"field","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"grave","card":"BP01-168"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"hidden","card":"BP01-168"}]);
    if available {
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"enemy-ex","card":"BP01-168"}]);
    }
    initial
}

#[test]
fn bp01_156_evolution_chooses_only_opposing_ex_and_banishes() {
    let (snapshot, keywords, mut input) = inputs();
    let (snapshot, companion) = evolution_companion(&snapshot, &mut input);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut rules: Value = serde_saphyr::from_str(
        &read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap(),
    )
    .unwrap();
    // The synthetic base extends the input; the existing fixture rebinds only its envelope.
    rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
    for available in [false, true] {
        let initial = ex_position(available);
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        let mut game = Game::from_selected_effects(
            selected,
            &initial,
            &Value::Null,
            &Value::Null,
            "real-ex-choice",
        )
        .unwrap();
        game.load_internal_card("synthetic-evolution", &companion)
            .unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution trigger")
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, "BP01-156");
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("EX choice")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("targets")
        };
        assert_eq!(domain.members.len(), usize::from(available));
        assert_eq!(domain.cardinality.minimum, Nat(u64::from(available)));
        let before = format!("{game:?}");
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![99]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid handles preserve the pending choice"
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(if available { vec![0] } else { vec![] }),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["ex"], json!([]));
        assert_eq!(
            state["P2"]["banish"],
            if available {
                json!(["enemy-ex"])
            } else {
                json!([])
            }
        );
        assert_eq!(state["P1"]["ex"], json!(["own-ex"]));
        assert_eq!(state["P2"]["field"], json!(["field"]));
        assert_eq!(state["P2"]["cemetery"], json!(["grave"]));
        assert_eq!(state["P2"]["hand"], json!(["hidden"]));
    }
}
