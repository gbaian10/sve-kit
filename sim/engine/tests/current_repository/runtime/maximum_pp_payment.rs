use super::*;

#[test]
fn phoenix_empress_spends_maximum_pp_to_return_acted() {
    let mut initial = position_with_source("BP06-058", "field");
    initial["players"]["P1"]["pp"] = json!({"current":1,"max":1});
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    drop(activate(&mut game, "P1"));
    checks(&mut game);
    drop(resolve_pending_card(&mut game, "BP06-058"));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["max"], 0);
    assert_eq!(after["P1"]["pp"]["current"], 0);
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["objects"]["source"]["acted"], true);
}
