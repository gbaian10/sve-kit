use super::*;
use sve_engine::game::internal::InputDomain;

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        assert!(
            matches!(q.domain, InputDomain::WardEntry { .. }),
            "Only the entrant’s Ward posture choice may interrupt this fixture."
        );
        step = game
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
    resolved(step);
}

fn tree(aura: bool, keyword: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = json!({"cost":{"pp":0},
    "targets":[{"bind":"recipient","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],
    "do":[
        {"grant":{"on":"this","keywords":[keyword]}},
        {"move":{"ref":"recipient","to":"field","result":"entered"}},
        {"if":{"cond":{"exists":{"in":{"follow":"entered"},"keyword":keyword}},"then":[{"life":{"who":"self","delta":2}}]}},
        {"life":{"who":"self","delta":1}}
    ]});
    let mut provider = source.clone();
    let mut entry = provider["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("activate");
    entry["static"] = json!({"effects":[{"grant":{"on":{"who":"self","zone":"field","kind":"follower"},"keywords":[keyword]}}]});
    provider["abilities"] = if aura { json!([entry]) } else { json!([]) };
    tree["cards"] = json!({"unit-follower":source,"unit-amulet":provider});
    tree
}

#[test]
fn followed_keyword_reads_only_successful_entry_and_current_effective_keyword() {
    for keyword in ["ward", "storm"] {
        for (aura, full) in [(true, false), (false, false), (true, true)] {
            let programs = lowerer().lower(&tree(aura, keyword)).unwrap();
            let mut initial = setup(1, json!([]));
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P2"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":"unit-follower"},{"id":"aura","card":"unit-amulet"}
            ]);
            if full {
                for i in 0_u8..3 {
                    initial["players"]["P1"]["zones"]["field"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":format!("filler-{i}"),"card":"unit-amulet"}));
                }
            }
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"recipient","card":"unit-follower"}]);
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "follow-keyword",
            )
            .unwrap();
            for (number, program) in programs {
                game.load_internal_card(&number, &program).unwrap();
            }
            let CardPlay::Awaiting(q) = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("target")
            };
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("cemetery")
            };
            let step = game
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            finish(&mut game, step);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["leader"]["life"],
                if aura && !full { 23_u8 } else { 21_u8 }
            );
            assert_eq!(
                view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("recipient")),
                !full
            );
            assert_eq!(view["objects"]["recipient"]["generation"], u8::from(!full));
        }
    }
}

#[test]
fn followed_keyword_rejects_unknown_keywords_results_filters_and_ex_producers() {
    for condition in [
        json!({"in":{"follow":"unknown"},"keyword":"ward"}),
        json!({"in":{"follow":"entered"},"keyword":"unknown_keyword"}),
        json!({"in":{"follow":"entered"},"keyword":"ward","name":"unit-follower"}),
    ] {
        let mut input = tree(false, "ward");
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][2]["if"]["cond"]["exists"] =
            condition;
        lowerer().lower(&input).unwrap_err();
    }
    let mut input = tree(false, "ward");
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["move"]["to"] =
        json!("ex");
    let mut producer = input.clone();
    producer["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .truncate(2);
    lowerer().lower(&producer).unwrap();
    lowerer().lower(&input).unwrap_err();
}
