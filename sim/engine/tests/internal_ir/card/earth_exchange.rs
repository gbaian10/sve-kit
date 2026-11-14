use super::*;
use sve_engine::catalog::SelectedEffects;
use sve_engine::game::internal::InputDomain;

fn program() -> WellFormedCardProgram {
    lowerer()
        .lower(&exchange::candidate(
            "unit-follower",
            json!({
                "optional_cost":{"earth_rite":2},"optional_cost_set":1
            }),
            false,
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn stack_selection(snapshot: &str, document: &str) -> SelectedEffects {
    let mut document: Value = serde_json::from_str(document).unwrap();
    document["cards"]
        .as_object_mut()
        .unwrap()
        .remove("unit-amulet");
    let face = "f:11111111111111111111111111111111";
    let card = "c:11111111111111111111111111111111";
    let author = json!({"status":"accepted","printings":["unit-amulet"],"input":{
        "admission":"direct","card_id":card,"card_no":"unit-amulet",
        "source_payload":{"card_id":card,"faces":[{
            "face_id":face,"revision_id":"stack-revision","ordinal":0_u8,"side":"front","cost":4_u8,
            "fields":[{"field":"effect","ordinal":null,"lines":[1_u8]}]
        }]},
        "tree":{"format":1_u8,"kind":"author_effect_set","scope":"shared","cards":{"unit-amulet":{
            "meta":{"dsl":"1.0","source":"direct","admission":"light",
                "written_by":[{"actor":{"who":"test","model":null},"date":"2026-11-14"}],
                "reviewed_by":[],"status":"draft","verified_by_exam":[],"rulings":[]},
            "faces":{face:{"abilities":[{"line":1_u8,"keywords":["stack"],
                "ports":{"imports":{},"exports":{}},"sources":[{
                    "admission":"light","owner":{"kind":"face_revision","revision_id":"stack-revision"},
                    "face_id":face,"field":"effect","ordinal":null,"line":1_u8,
                    "scope":"ability_0","covers":[1_u8]
                }]}]}}
        }}}
    }});
    Catalog::from_current_selection(
        snapshot,
        include_str!("../../../../../authored/rules/keywords.yaml"),
        &json!({"documents":[["unit.yaml",document.to_string()]],"authors":[author]}),
    )
    .unwrap()
}

fn board(stack: u8, pp: u8) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"soil","card":"unit-amulet","state":{"counters":{"stack_counter":stack}}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let (snapshot, _registry, document) = catalog_documents(&json!("4"));
    let selected = stack_selection(&snapshot, &document);
    let mut game = Game::from_selected_effects(
        selected,
        &initial,
        &Value::Null,
        &Value::Null,
        "earth-exchange",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program())
        .unwrap();
    game
}

#[test]
fn earth_exchange_sets_pp_only_after_complete_keyword_payment_and_can_be_declined() {
    for (count, pp, accept) in [
        (2_u8, 1_u8, true),
        (3, 1, true),
        (2, 4, false),
        (1, 1, true),
    ] {
        let mut game = board(count, pp);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) =
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
        else {
            panic!("optional exchange");
        };
        assert_eq!(question.domain, InputDomain::OptionalCost);
        let result =
            game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(accept));
        if count < 2 && accept {
            result.unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"],
                before["P1"]["pp"]
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["s"])
            );
            continue;
        }
        let mut play = result.unwrap();
        if accept {
            let CardPlay::Awaiting(material) = play else {
                panic!("Stack material");
            };
            let InputDomain::CostMaterials(domain) = &material.domain else {
                panic!("material domain");
            };
            assert_eq!(domain.members.len(), 1);
            play = game
                .answer_internal_card(
                    material.actor,
                    material.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap();
        }
        assert!(matches!(play, CardPlay::Entered(_)));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            if accept { pp - 1 } else { pp - 4 }
        );
        if accept && count == 2 {
            assert_eq!(view["P1"]["cemetery"], json!(["soil"]));
        } else {
            assert_eq!(
                view["objects"]["soil"]["counters"]["スタックカウンター"],
                if accept { count - 2 } else { count }
            );
        }
    }
}

#[test]
fn earth_exchange_ir_retains_the_keyword_and_rejects_cross_material_payment() {
    for cross_material in [false, true] {
        let mut raw = program().card().clone();
        let CostSpec::Ordered { steps } = &mut raw.card_play_costs[0]
            .node
            .exchange
            .as_mut()
            .unwrap()
            .node
            .cost
        else {
            panic!("ordered payment");
        };
        if cross_material {
            let Action::Counter { holder, .. } = &mut steps[0].instruction.action else {
                panic!("counter cost");
            };
            *holder = Holder::Object {
                object: ObjectOperand::SourceHolder {},
            };
        } else {
            steps[0].keyword = None;
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
}
