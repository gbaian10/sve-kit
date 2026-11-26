use super::event_cemetery::finish_checks;
use super::*;

const P1: PlayerId = PlayerId { seat: 1 };
const P2: PlayerId = PlayerId { seat: 2 };

fn real_game(initial: &Value, effects: &SelectedEffects) -> Game {
    Game::from_selected_effects(
        effects.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-discard-triggers",
    )
    .unwrap()
}

fn answer(
    game: &mut Game,
    step: CardPlay,
    select: impl FnOnce(&InputDomain) -> Answer,
) -> CardPlay {
    let CardPlay::Awaiting(question) = step else {
        panic!("expected a printed ability decision");
    };
    game.answer_internal_card(question.actor, question.token, select(&question.domain))
        .unwrap()
}

fn discard_everything(game: &mut Game, actor: PlayerId, side: &str) -> CardPlay {
    let mut tree = expanded::tree().clone();
    let mut row = tree["cards"]["BP01-041"].clone();
    let mut entry = row["abilities"][0].clone();
    entry["activate"] = json!({"cost":{"pp":0_u8},"do":[{"discard":{"who":side,"all":true}}]});
    row["abilities"] = json!([entry]);
    tree["cards"] = json!({"producer":row});
    let discard = lowerer().lower(&tree).unwrap().remove("producer").unwrap();
    game.load_internal_card("producer", &discard).unwrap();
    game.activate_internal_card("producer", AbilityId(1), actor)
        .unwrap()
}

fn producer_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, inputs) = producer_inputs();
        repository::selection_with_rules_from(&snapshot, &keywords, &inputs)
    })
}

fn pending_card(game: &mut Game, number: &str, count: usize) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected printed pending abilities");
    };
    assert_eq!(options.len(), count, "only matching discards may trigger");
    let occurrence = options.iter().find(|option| option.card == number).unwrap();
    assert_eq!(
        occurrence.event, "Discard",
        "the printed ability observes discard"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(occurrence.handle),
    )
    .unwrap()
}

#[test]
fn fediel_deals_damage_once_for_each_dark_card_in_a_mixed_discard() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"observer","card":"CP05-GBF47"},{"id":"producer","card":"producer"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"dark1","card":"CP05-074"}, {"id":"other","card":"BP01-171"},
        {"id":"dark2","card":"CP05-076"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut board = real_game(&initial, producer_selection());
    resolved(discard_everything(&mut board, P1, "self"));
    for remaining in [2, 1] {
        resolved(pending_card(&mut board, "CP05-GBF47", remaining));
    }
    finish_checks(&mut board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "Ferry cannot respond to the non-dark producer that discarded her"
    );
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(view["P1"]["cemetery"], json!(["dark1", "other", "dark2"]));
}

#[test]
fn ferry_enters_when_her_own_dark_amulet_discards_her_as_a_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"CP05-079"}]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"ferry","card":"CP05-GBF49"},{"id":"other","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-171"},{"id":"next","card":"BP01-171"}]);
    let selected = producer_selection();
    let mut board = real_game(&initial, selected);
    let ability = selected.current["CP05-079"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 2)
        .unwrap()
        .node
        .id;
    let activation = board.activate_internal_card("amulet", ability, P1).unwrap();
    let look = answer(&mut board, activation, |domain| {
        let InputDomain::CostMaterials(domain) = domain else {
            panic!("expected the private discard payment");
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("CP05-GBF49"));
        Answer::CostMaterials(vec![domain.members[0].handle])
    });
    let order = answer(&mut board, look, |domain| {
        assert!(matches!(domain, InputDomain::Integer(_)));
        Answer::Integer(Nat(1))
    });
    resolved(answer(&mut board, order, |domain| {
        let InputDomain::Order(cards) = domain else {
            panic!("expected a private deck order");
        };
        assert_eq!(cards.len(), 1);
        Answer::Order(vec![cards[0].handle])
    }));
    finish_checks(&mut board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed discard response");
    };
    let ferry = options
        .iter()
        .find(|option| option.card == "CP05-GBF49")
        .unwrap();
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(ferry.handle),
            )
            .unwrap(),
    );
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["amulet", "ferry"]));
    assert_eq!(view["P1"]["deck"], json!(["next", "top"]));
    assert_eq!(view["P1"]["hand"], json!(["other"]));
}

#[test]
fn celeste_magna_caps_matching_discards_in_the_opponents_turn() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"CP05-P59"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"dark1","card":"CP05-074"},{"id":"dark2","card":"CP05-074"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"producer","card":"producer"},{"id":"enemy","card":"BP01-171"}]);
    let mut board = real_game(&initial, producer_selection());
    let discard = discard_everything(&mut board, P2, "opp");
    resolved(answer(&mut board, discard, |domain| {
        let InputDomain::TriggerQuota(domain) = domain else {
            panic!("expected a choice between the simultaneous matching discards");
        };
        assert_eq!(domain.candidates.len(), 2);
        Answer::TriggerQuota(domain.candidates[0].handle)
    }));
    let targets = pending_card(&mut board, "CP05-P59", 1);
    resolved(answer(&mut board, targets, |domain| {
        let InputDomain::Targets(domain) = domain else {
            panic!("expected a formal enemy follower target");
        };
        let target = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-171"))
            .unwrap();
        Answer::Targets(vec![target.handle])
    }));
    finish_checks(&mut board);
    assert!(board.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        1_u8
    );
}
