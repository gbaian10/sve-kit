use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

#[expect(
    clippy::panic,
    reason = "Unexpected lowered shapes invalidate this synthetic fixture."
)]
fn programs(depart: bool) -> BTreeMap<String, WellFormedCardProgram> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut observer = tree["cards"]["BP01-041"].clone();
    let mut observer_ability = observer["abilities"][0].clone();
    observer_ability.as_object_mut().unwrap().remove("activate");
    observer_ability["auto"] = json!({
        "on":{"moved":{"from":{"who":"self","zone":"field"},"to":"cemetery","sel":{"kind":"follower"}}},
        "do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]
    });
    observer["abilities"] = json!([observer_ability]);
    let mut producer = tree["cards"]["BP01-041"].clone();
    let mut steps = vec![
        json!({"move":{"ref":{"who":"self","zone":"field","kind":"follower","name":"unit-material"},"to":"cemetery"}}),
        json!({"buff":{"on":"reader","power":3}}),
    ];
    if depart {
        steps.push(json!({"move":{"ref":"reader","to":"cemetery"}}));
    }
    let mut producer_ability = producer["abilities"][0].clone();
    producer_ability["activate"] = json!({
        "cost":{"pp":1},
        "targets":[{"bind":"reader","from":{"who":"self","zone":"field","kind":"follower","name":"unit-follower"},"n":1}],
        "do":steps
    });
    producer["abilities"] = json!([producer_ability]);
    tree["cards"] = json!({"unit-follower":observer,"unit-amulet":producer});
    let mut programs = lowerer().lower(&tree).unwrap();
    // This probe isolates SourceInformation without extending scalar DSL source reads.
    let mut observer = programs["unit-follower"].card().clone();
    let Node::Instruction { instruction } =
        &mut observer.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("lowered observer damage")
    };
    let Action::Damage { amount, .. } = &mut instruction.action else {
        panic!("lowered damage action")
    };
    *amount = DamageAmount::Uniform {
        amount: source_information::source_read(
            &ObjectView::SourceInformation {},
            CharacteristicView::Effective,
            Some(Stat::Attack),
        ),
    };
    programs.insert(
        "unit-follower".into(),
        WellFormedCardProgram::check(observer).unwrap(),
    );
    programs
}

fn material_catalog() -> Arc<Catalog> {
    let (mut snapshot, keywords, document) = catalog_documents(&json!("1"));
    let row = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["number"] == "unit-follower")
        .unwrap();
    let mut material = row;
    material["number"] = json!("unit-material");
    material["faces"][0]["name"] = json!("unit-material");
    snapshot.push('\n');
    snapshot.push_str(&material.to_string());
    let extra = json!({"format":1,"kind":"effect_set","cards":{"unit-material":{"status":"complete","abilities":[]}}}).to_string();
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &keywords,
            &[
                ("unit.yaml".into(), document),
                ("material.yaml".into(), extra),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn an_exit_observers_source_stays_live_until_its_own_departure() {
    let catalog = material_catalog();
    for depart in [false, true] {
        let programs = programs(depart);
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"observer","card":"unit-follower"},
            {"id":"material","card":"unit-material"},
            {"id":"producer","card":"unit-amulet"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "exit-source-lifetime",
        )
        .unwrap();
        for number in ["unit-follower", "unit-amulet"] {
            game.load_internal_card(number, &programs[number]).unwrap();
        }
        game.load_internal_card("unit-material", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "producer",
                programs["unit-amulet"].card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("formal reader target")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("reader domain")
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        for remaining in (1..=if depart { 2_usize } else { 1_usize }).rev() {
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("exit observer")
            };
            assert_eq!(options.len(), remaining);
            resolved(
                game.answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if depart { 10_u8 } else { 15_u8 },
            "the other object's exit never freezes the observer at two; its own later departure retains five"
        );
    }
}
