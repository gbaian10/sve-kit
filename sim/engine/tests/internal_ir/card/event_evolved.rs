use super::*;
use sve_engine::game::internal::InputDomain;

fn programs(case: u8) -> (WellFormedCardProgram, WellFormedCardProgram) {
    let mut tree = fanfare::expanded::tree().clone();
    let template = tree["cards"]["BP08-038"].clone();
    let mut ability = template["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["line"] = json!(1);
    let mut intrinsic = ability.clone();
    intrinsic["evolve"] = json!({"cost":{"pp":1}});
    let header = if case == 0 {
        json!({"who":"self","sel":{"kind":"follower"}})
    } else {
        json!({"ref":"this"})
    };
    ability["auto"] =
        json!({"on":{"evolved":header},"do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]});
    let mut base = template.clone();
    base["abilities"] = if case == 2 {
        json!([intrinsic])
    } else {
        let mut observer = ability.clone();
        observer["line"] = json!(2);
        json!([intrinsic, observer])
    };
    tree["cards"] = json!({"unit-follower":base});
    if case == 2 {
        let mut form = template;
        form["abilities"] = json!([ability]);
        tree["cards"]["unit-form"] = form;
    }
    let mut lowered = lowerer().lower(&tree).unwrap();
    let base = lowered.remove("unit-follower").unwrap();
    let form = lowered.remove("unit-form").unwrap_or_else(|| card(vec![]));
    (base, form)
}

#[test]
fn evolution_observers_use_the_new_effective_print_and_match_itself_precisely() {
    for case in 0..3 {
        let (base, form) = programs(case);
        let mut game = evolution::position(1, &base, &form);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let pending = game.internal_pending_choice().unwrap();
        if case == 1 {
            assert!(
                pending.is_none(),
                "the old self-observer is gone, and the other unevolved copy is not the subject"
            );
            continue;
        }
        let question = pending.unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("one effective observer")
        };
        assert_eq!(
            options.len(),
            1,
            "the replaced print never contributes another occurrence"
        );
        assert_eq!(
            options[0].card,
            if case == 0 {
                "unit-follower"
            } else {
                "unit-form"
            }
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            19_u8
        );
    }
}
