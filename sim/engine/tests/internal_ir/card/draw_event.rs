use super::*;

fn tree(on: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["auto"] = json!({"on":on,"do":[{"buff":{"on":"this","power":1,"hp":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn own_non_start_draw_lowers_to_per_fact_after_entry_on_the_existing_draw_event() {
    let programs = lowerer()
        .lower(&tree(&json!({"drew":{"who":"self","not_phase":"start"}})))
        .unwrap();
    let card = programs["unit-follower"].card();
    let trigger = card.abilities[0].node.trigger.as_ref().unwrap();
    assert!(matches!(
        trigger.on,
        TriggerCondition::Event {
            pattern: EventPattern::Draw { .. }
        }
    ));
    assert_eq!(trigger.view, EventSnapshotPolicy::AfterEntry);
    assert_eq!(trigger.aggregation, TriggerAggregation::PerFact {});
}

#[test]
fn draw_headers_keep_other_players_phases_filters_and_missing_exclusions_closed() {
    for payload in [
        json!({"who":"opp","not_phase":"start"}),
        json!({"who":"self","not_phase":"main"}),
        json!({"who":"self","not_phase":"end"}),
        json!({"who":"self"}),
        json!({"not_phase":"start"}),
        json!({"who":"self","not_phase":"start","by":"ability"}),
    ] {
        lowerer()
            .lower(&tree(&json!({"drew":payload})))
            .unwrap_err();
    }
}
