use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, ub};
use super::*;

#[test]
fn nephi_nera_discounts_only_original_one_cost_ex_followers_and_observes_other_ubs() {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP04-004"},
        {"id":"hand","card":"CP04-095"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"one","card":"CP04-031"},
        {"id":"two","card":"CP04-027"},
        {"id":"other_trait","card":"BP01-042"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"CP04-089"},
        {"id":"second","card":"BP01-042"},
        {"id":"third","card":"BP01-042"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8}}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let fanfare = pending(&mut game, "CP04-004");
    ub(&resolved(finish(&mut game, fanfare)));
    finish_checks(&mut game);
    let after_entry = game.projection(View::Referee).unwrap();
    assert_eq!(after_entry["objects"]["enemy"]["hp"], 6_u8);
    assert_eq!(
        after_entry["P1"]["ex"],
        json!(["one", "two", "other_trait", "first"])
    );
    let before_costs = after_entry["P1"]["pp"]["current"].as_u64().unwrap();
    for id in ["one", "two", "other_trait"] {
        let play = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
        meal_race::settle(&mut game);
    }
    let after_costs = game.projection(View::Referee).unwrap();
    assert_eq!(after_costs["P1"]["pp"]["current"], before_costs - 3);
    assert_eq!(after_costs["P1"]["ex"], json!(["first"]));

    let entry = game
        .play_internal_card("first", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(finish(&mut game, entry), CardPlay::Entered(_)));
    let fanfare = pending(&mut game, "CP04-089");
    ub(&resolved(finish(&mut game, fanfare)));
    finish_checks(&mut game);
    let observer = pending(&mut game, "CP04-004");
    resolved(finish(&mut game, observer));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["second"])
    );
}
