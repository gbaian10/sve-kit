use super::event_cemetery::finish_checks;
use super::*;

fn program() -> &'static WellFormedCardProgram {
    static PROGRAM: OnceLock<WellFormedCardProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let mut card = tree["cards"]["BP08-038"].clone();
        let mut observer = card["abilities"][0].clone();
        observer.as_object_mut().unwrap().remove("fanfare");
        observer["auto"] = json!({"on":{"ub_activated":{"who":"self","holder":{
            "who":"self","zone":"field","kind":"follower","other":true
        }}},"do":[{"buff":{"on":{"event":"holder"},"power":1_u8,"hp":1_u8}}]});
        card["abilities"] = json!([observer]);
        tree["cards"] = json!({"producer":card});
        lowerer().lower(&tree).unwrap().remove("producer").unwrap()
    })
}

#[test]
fn erikos_real_ub_binds_its_holder_for_the_cp04_112_style_observer() {
    // The unadmitted CP04-112 card is not replaced with an empty authored program.
    let mut initial = event_subject_capture::position("CP04-089", true);
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    let mut game = event_subject_capture::prepared_program(&initial, program());
    let entry = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(
        union_burst_batch1::finish(&mut game, entry),
        CardPlay::Entered(_)
    ));
    finish_checks(&mut game);
    let before = game.projection(View::Referee).unwrap();
    let fanfare = union_burst_batch1::pending(&mut game, "CP04-089");
    let result = union_burst_batch1::finish(&mut game, fanfare);
    union_burst_batch1::ub(&resolved(result));
    let observer = union_burst_batch1::pending(&mut game, "producer");
    resolved(union_burst_batch1::finish(&mut game, observer));
    finish_checks(&mut game);
    let after = game.projection(View::Referee).unwrap();
    for stat in ["power", "hp"] {
        assert_eq!(
            after["objects"]["source"][stat].as_u64().unwrap(),
            before["objects"]["source"][stat]
                .as_u64()
                .unwrap()
                .checked_add(1)
                .unwrap()
        );
        assert_eq!(
            after["objects"]["producer"][stat],
            before["objects"]["producer"][stat]
        );
    }
    assert_eq!(
        after["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
        1_u8
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}
