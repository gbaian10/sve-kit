#![expect(
    clippy::panic,
    reason = "The fixture must ask only its declared public targets."
)]
use super::*;
use alloc::collections::BTreeSet;

// Population changes must not enqueue unrelated entry abilities.
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        snapshot.push_str(
            &json!({"number":"condition-fuel","faces":[{
                "name":"population fixture","card_class":"ニュートラル","card_type":"フォロワー",
                "cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]
            }]})
            .to_string(),
        );
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "condition-fuel.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{
                "condition-fuel":{"status":"complete","abilities":[]}
            }})
            .to_string()
        ]));
        repository::select(&snapshot, &keywords, &input).unwrap()
    })
}

fn mover() -> &'static WellFormedCardProgram {
    static PROGRAM: OnceLock<WellFormedCardProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let mut card = tree["cards"]["BP08-038"].clone();
        let mut template = card["abilities"][0].clone();
        template.as_object_mut().unwrap().remove("fanfare");
        let mut abilities = Vec::new();
        for (line, from, to, count) in [
            (1_u8, "field", "cemetery", 1_u8),
            (2, "cemetery", "field", 1),
            (3, "cemetery", "banish", 8),
        ] {
            let mut entry = template.clone();
            entry["line"] = json!(line);
            let step = if to == "banish" {
                json!({"banish":"fuel"})
            } else if to == "field" {
                json!({"move":{"ref":"fuel","to":"field","result":"entered"}})
            } else {
                json!({"move":{"ref":"fuel","to":to}})
            };
            entry["activate"] = json!({"cost":{"pp":0_u8},
                "targets":[{"bind":"fuel","from":{"who":"self","zone":from,"kind":"follower"},"n":count}],
                "do":[step]
            });
            abilities.push(entry);
        }
        card["abilities"] = json!(abilities);
        tree["cards"] = json!({"producer":card});
        lowerer().lower(&tree).unwrap().remove("producer").unwrap()
    })
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"base","card":"BP08-081","state":{"evolved":true,"evolved_with":"form","power":4_u8,"hp":4_u8,"max_hp":4_u8}},
        {"id":"fuel","card":"condition-fuel"}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"BP08-082"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..14)
            .map(|index| json!({"id":format!("buried-{index}"),"card":"condition-fuel"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn move_fuel(game: &mut Game, line: u32, count: usize) {
    let ability = mover()
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("producer", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal public fuel targets")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("fuel target domain")
    };
    let chosen: Vec<_> = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("condition-fuel"))
        .take(count)
        .map(|member| member.handle)
        .collect();
    assert_eq!(
        chosen.len(),
        count,
        "only inert population fixtures may move"
    );
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(chosen))
            .unwrap(),
    );
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn assert_keywords(game: &Game, expected: &[&str]) {
    let view = game.projection(View::Referee).unwrap();
    let keywords: BTreeSet<String> =
        serde_json::from_value::<Vec<String>>(view["objects"]["base"]["keywords"].clone())
            .unwrap()
            .into_iter()
            .collect();
    assert_eq!(
        keywords,
        expected
            .iter()
            .map(|keyword| (*keyword).to_owned())
            .collect(),
        "each declaration follows its own live cemetery threshold"
    );
    assert_eq!(
        view["objects"]["base"]["generation"], 0_u8,
        "only the population changes"
    );
}

#[test]
fn bp08_082_keeps_one_source_ability_and_recomputes_both_necrocharge_thresholds() {
    let current = &fixture().current["BP08-082"];
    assert_eq!(
        current.program.card().abilities.len(),
        2,
        "Ward and one compound static ability"
    );
    let static_ability = current
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 2)
        .unwrap();
    assert_eq!(static_ability.node.declarations.len(), 2);
    for index in 0_u8..2 {
        let path = format!(
            "{}/static/effects/{index}/if/then/0/grant",
            static_ability.source.author_path
        );
        let source = current.source_at(&path).unwrap();
        assert_eq!(source.source.card_id, current.card_id);
        assert_eq!(
            source.source.line, 2,
            "both clauses cover the same complete source line"
        );
    }
    let mut game = with_selected_producer_program(&position(), mover(), fixture());
    game.load_internal_card(
        "condition-fuel",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    assert_keywords(&game, &["守護", "指定攻撃"]);
    move_fuel(&mut game, 1, 1);
    assert_keywords(&game, &["守護", "指定攻撃", "疾走", "必殺"]);
    move_fuel(&mut game, 2, 1);
    assert_keywords(&game, &["守護", "指定攻撃"]);
    move_fuel(&mut game, 3, 8);
    assert_keywords(&game, &["守護"]);
    move_fuel(&mut game, 1, 1);
    assert_keywords(&game, &["守護", "指定攻撃"]);
    move_fuel(&mut game, 2, 1);
    assert_keywords(&game, &["守護"]);
}
