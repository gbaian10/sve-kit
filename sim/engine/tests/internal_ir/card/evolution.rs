use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

#[expect(
    clippy::panic,
    reason = "Unexpected intrinsic fixture shapes must fail the test."
)]
fn map_steps(nodes: &mut [Located<Node>], span: &SourceSpan, origin: &AbilitySource) {
    for step in nodes {
        step.source = span.clone();
        match &mut step.node {
            Node::Instruction { instruction } => instruction.source = origin.clone(),
            Node::Control {
                control: Control::EachObject { body, .. },
            } => map_steps(&mut body.steps, span, origin),
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {
                panic!("unexpected intrinsic projection")
            }
        }
    }
}

#[expect(
    clippy::panic,
    reason = "Unexpected intrinsic costs must fail the test."
)]
fn relabel(mut ability: Located<Ability>, card: &str, line: u64) -> Located<Ability> {
    let mut span = source(card);
    span.fragment = None;
    span.line = u32::try_from(line).unwrap();
    let printed = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(line),
    };
    match &mut ability.node.source.origin {
        AbilityOrigin::Keyword { origin, .. } => **origin = printed,
        origin @ (AbilityOrigin::Printed { .. } | AbilityOrigin::Granted { .. }) => {
            *origin = printed;
        }
    }
    ability.source = span.clone();
    let origin = ability.node.source.clone();
    if let Some(play) = &mut ability.node.play {
        let CostSpec::Ordered { steps } = &mut play.costs else {
            panic!("intrinsic costs")
        };
        for step in steps {
            step.instruction.source = origin.clone();
        }
    }
    map_steps(
        &mut ability.node.body.as_mut().unwrap().steps,
        &span,
        &origin,
    );
    ability
}

fn position(seat: u32, base: &WellFormedCardProgram, form: &WellFormedCardProgram) -> Game {
    let snapshot = [("unit-follower","フォロワー","1","2","3"),("unit-form","エボルヴフォロワー","-","4","5"),("unit-spell","スペル","1","-","-")]
        .iter().map(|(number,kind,cost,power,hp)| json!({"number":number,"faces":[{"name":if *number=="unit-form" {"unit-follower"} else {number},"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":power,"hp":hp,"traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document=json!({"format":1,"kind":"effect_set","cards":{
        "unit-follower":{"status":"complete","abilities":[]},"unit-form":{"status":"complete","abilities":[]},"unit-spell":{"status":"complete","abilities":[]}
    }}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("evolution.yaml".into(), document)])
            .unwrap(),
    );
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([{"id":"d","card":"unit-spell"}]));
    initial["turn"]["active"] = json!(actor);
    initial["turn"]["elapsed_turns"][actor] = json!(7);
    initial["players"][actor]["sep"] = json!(1);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"u","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-form"}]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "evolution-seed",
    )
    .unwrap();
    game.load_internal_card("unit-follower", base).unwrap();
    game.load_internal_card("unit-form", form).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn programs() -> (WellFormedCardProgram, WellFormedCardProgram) {
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower(fanfare::expanded::tree())
        .unwrap();
    let intrinsic = relabel(
        lowered["synthetic-evolution"].card().abilities[0].clone(),
        "unit-follower",
        1,
    );
    let sink = damage(
        8,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 80),
        },
        99,
    );
    let base = card(vec![
        intrinsic,
        ability(7, vec![draw(7, 0)]),
        ability(8, vec![sink]),
    ]);
    let mut evolved = relabel(
        lowered["BP20-R14"].card().abilities[0].clone(),
        "unit-form",
        1,
    );
    evolved.node.id = AbilityId(7);
    evolved.node.body.as_mut().unwrap().steps.extend([
        source_information::source_hit(
            10,
            source_information::source_read(
                &ObjectView::SourceInformation {},
                CharacteristicView::Original,
                None,
            ),
        ),
        source_information::source_hit(
            11,
            source_information::source_read(
                &ObjectView::SourceInformation {},
                CharacteristicView::Effective,
                Some(Stat::Attack),
            ),
        ),
    ]);
    map_steps(
        &mut evolved.node.body.as_mut().unwrap().steps,
        &evolved.source,
        &evolved.node.source,
    );
    let lastword = relabel(
        triggers::lastword(ability(
            8,
            vec![source_information::source_hit(12, Expr::Literal(Int(2)))],
        )),
        "unit-form",
        8,
    );
    (base, card(vec![evolved, lastword]))
}

#[test]
fn bp20_r14_evolve_draw_fragment_uses_new_print_instance_and_real_super_facet() {
    let (base, form) = programs();
    for seat in [1, 2] {
        let mut game = position(seat, &base, &form);
        let prior = resolved(
            game.activate_internal_card("s", AbilityId(7), PlayerId { seat })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let super_evolution = u64::from(seat == 2);
        let step=game.internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1,"ep":0,"sep":super_evolution},"kind":"forged-unused"})).unwrap();
        assert_eq!(step.outcome, "resolved");
        assert!(
            step.events
                .iter()
                .any(|event| event["kind"] == "公開" && event["object"] == "e")
        );
        let question = game.internal_pending_choice().unwrap().unwrap();
        assert!(
            matches!(&question.domain,InputDomain::Pending(options) if options.len()==1 && options[0].card=="unit-form" && options[0].event.contains(if seat==1 {"Evolved"} else {"SuperEvolved"}))
        );
        let evolved = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
                .unwrap(),
        );
        assert_eq!(evolved.source.holder, prior.source.holder);
        assert_eq!(evolved.source.definition, prior.source.definition);
        assert_ne!(evolved.source.instance, prior.source.instance);
        assert_eq!(evolved.source.holder.generation, Generation(0));
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view[opponent]["leader"]["life"], 15 - super_evolution);
        assert_eq!(
            view[if seat == 1 { "P1" } else { "P2" }]["pp"]["current"],
            1
        );
        assert!(
            evolved
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Draw(_)))
        );
        game.resume_internal_card().unwrap();
        resolved(
            game.activate_internal_card("u", AbilityId(8), PlayerId { seat })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let lastword = game.internal_pending_choice().unwrap().unwrap();
        let lastword = resolved(
            game.answer_internal_card(lastword.actor, lastword.token, Answer::Pending(1))
                .unwrap(),
        );
        assert_eq!(lastword.source.holder, evolved.source.holder);
        assert_eq!(lastword.source.definition, AbilityId(8));
        assert_eq!(
            game.projection(View::Referee).unwrap()[opponent]["leader"]["life"],
            13 - super_evolution
        );
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
