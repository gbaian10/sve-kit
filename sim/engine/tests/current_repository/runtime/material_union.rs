#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed material scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP07-117" | "BP07-019" | "BP07-T03" | "CP02-T01" | "BP03-T02")));
        // Blank companions isolate Bayleon's payment; Carrot uses the actual Tree token.
        for (number, name, kind, traits) in [
            ("union-tree", "ナテラの大樹", "アミュレット", vec![]),
            ("union-wrong", "other", "アミュレット", vec![]),
            ("union-ally", "ally", "フォロワー", vec!["自然"]),
            ("union-enemy", "enemy", "フォロワー", vec![]),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":traits,"cost":"1","power":if kind=="フォロワー" {"5"} else {"-"},"hp":if kind=="フォロワー" {"20"} else {"-"},"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-union",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["union-tree", "union-wrong", "union-ally", "union-enemy"] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}

fn material_answer(game: &mut Game, question: &DecisionView, selected: u64) -> Answer {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("materials")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the two own named trees are candidates"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(1),
        "one tree pays the entire cost"
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![0, 1]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "excess material input is atomic"
    );
    Answer::CostMaterials(vec![selected])
}

fn assert_tree_payment(game: &mut Game, paid: &str, from_field: bool) {
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["objects"].get(paid).is_none(),
        "the actual paid token is erased"
    );
    assert_eq!(
        state["P1"]["banish"],
        json!([]),
        "erasure still completes the banishment fee"
    );
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap();
    if from_field {
        let question = pending.unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("Tree departure")
        };
        assert_eq!(
            options.len(),
            1,
            "only the departed field Tree waits after Carrot's complete effect"
        );
        assert_eq!(
            options[0].card, "BP07-T03",
            "Q1045 keeps the Tree's printed departure ability"
        );
    } else {
        assert!(pending.is_none(), "the EX Tree did not leave the field");
    }
}

#[test]
fn carrot_lastword_can_pay_from_either_own_zone_and_never_offers_foreign_or_wrong_names() {
    for (printing, selected, paid) in [("BP07-117", 0_u64, "field-tree"), ("PR-232", 1, "ex-tree")]
    {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing,"state":{"acted":true}},{"id":"field-tree","card":"BP07-T03"},{"id":"wrong","card":"union-wrong"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-tree","card":"BP07-T03"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"replacement","card":"BP07-117"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"union-enemy"},{"id":"foreign-field","card":"BP07-T03"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign-ex","card":"BP07-T03"}]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap();
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("Lastword")
        };
        assert_eq!(options.len(), 1, "only the actual departed carrot triggers");
        let mut play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(_) => material_answer(&mut game, &question, selected),
                InputDomain::Search(domain) => {
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
                }
                _ => panic!("unexpected Lastword question"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"], 11_u8,
            "the printed recovery follows search"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "the ordered PP fee is paid once"
        );
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("replacement")),
            "the same-name search enters its real follower"
        );
        assert_eq!(
            state["P2"]["ex"],
            json!(["foreign-ex"]),
            "foreign EX cannot pay"
        );
        assert!(
            state["P2"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("foreign-field")),
            "foreign field cannot pay"
        );
        assert_tree_payment(&mut game, paid, selected == 0);
    }
}

#[test]
fn bayleon_banishes_two_own_trees_and_the_default_field_cost_cannot_borrow_opponents() {
    for own_count in [1_u64, 2] {
        let mut initial = draw_position();
        let mut field = vec![
            json!({"id":"source","card":"BP07-019"}),
            json!({"id":"ally","card":"union-ally"}),
        ];
        field.extend((0..own_count).map(|i| json!({"id":format!("fee-{i}"),"card":"union-tree"})));
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"foreign","card":"union-tree"}]);
        let mut game = game(&initial);
        let ability = prepared().current["BP07-019"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let before = game.projection(View::Referee).unwrap();
        let result = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .and_then(|play| {
                let target = awaiting(play);
                let InputDomain::Targets(domain) = &target.domain else {
                    panic!("target")
                };
                let ally = domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("union-ally"))
                    .unwrap()
                    .handle;
                game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![ally]))
            });
        if own_count == 1 {
            result.unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "failed activation is atomic"
            );
            continue;
        }
        let materials = awaiting(result.unwrap());
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("materials")
        };
        assert_eq!(domain.members.len(), 2, "only own field trees are offered");
        resolved(
            game.answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![0, 1]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["banish"].as_array().unwrap().len(),
            2,
            "both materials are consumed"
        );
        assert_eq!(
            state["objects"]["ally"]["power"], 7_u8,
            "the printed buff resolves after payment"
        );
        assert_eq!(
            state["P2"]["field"],
            json!(["foreign"]),
            "the opponent's tree remains"
        );
    }
}
