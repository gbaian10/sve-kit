use super::*;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{
    CardCostQuote, CostGroupKind, CostLine, CostSummary, PendingCost, PpQuote, QuoteBasis,
};

pub(super) fn preview(game: &Game, viewer: Viewer) -> CardCostQuote {
    game.observe(viewer)
        .unwrap()
        .costs
        .cards
        .into_iter()
        .next()
        .unwrap()
}

pub(super) fn active(game: &Game) -> CostSummary {
    assert!(
        game.observe(Viewer::P2)
            .unwrap()
            .costs
            .current_play
            .is_none(),
        "the opponent cannot inspect a private play's cost state"
    );
    game.observe(Viewer::P1)
        .unwrap()
        .costs
        .current_play
        .unwrap()
        .cost
}

fn board(cost: &Value, hidden: &str) -> Game {
    board_with(cost, hidden, false)
}

fn board_with(cost: &Value, hidden: &str, overflow: bool) -> Game {
    let mut initial = setup(
        1,
        json!([
            {"id":"deck-a","card":"unit-follower"},
            {"id":"deck-b","card":"unit-spell"},
            {"id":"deck-c","card":"unit-amulet"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"secret","card":hidden}]);
    let mut game = Game::new(
        catalog_with_cost(cost),
        &initial,
        &Value::Null,
        &Value::Null,
        "cost-observation",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for card_no in ["unit-follower", "unit-spell", "unit-amulet"] {
        let program = if overflow && card_no == "unit-follower" {
            let span = source(card_no);
            WellFormedCardProgram::check(CardProgram {
                abilities: vec![],
                card_play_costs: vec![Located {
                    source: span.clone(),
                    node: FacePlayCost {
                        face: FaceId("0".into()),
                        additional: None,
                        exchange: None,
                        modifiers: vec![Located {
                            source: span,
                            node: PlayCostModifier {
                                condition: None,
                                change: CostChange::Delta {
                                    amount: Expr::Literal(Int(1)),
                                },
                            },
                        }],
                    },
                }],
            })
            .unwrap()
        } else {
            empty.clone()
        };
        game.load_internal_card(card_no, &program).unwrap();
    }
    game
}

#[test]
fn quotes_are_read_only_and_do_not_depend_on_the_opponents_hidden_configuration() {
    let mut game = board(&json!("1"), "unit-spell");
    let other = board(&json!("1"), "unit-amulet");
    let before = exact_authority(&game);
    let mut untouched = game.clone();
    let original = game.observe(Viewer::P1).unwrap();
    for _ in 0..3 {
        assert_eq!(original, game.observe(Viewer::P1).unwrap());
    }
    assert_eq!(before, exact_authority(&game));
    assert_eq!(original, other.observe(Viewer::P1).unwrap());
    assert_eq!(original.costs.cards.len(), 1);
    assert_eq!(
        original.costs.cards[0].cost.basis,
        QuoteBasis::CurrentContext
    );
    assert_eq!(original.costs.cards[0].cost.total_pp, Some(1));
    let shuffle = checked(vec![instruction(
        1,
        Action::Shuffle {
            deck: PlayerRef::Controller {},
            scope: ShuffleScope::FullZone {},
            randomness: RandomnessPolicy::FreshRequest,
        },
    )]);
    game.execute_internal(&shuffle, &source("synthetic"), context(1))
        .unwrap();
    untouched
        .execute_internal(&shuffle, &source("synthetic"), context(1))
        .unwrap();
    assert_eq!(exact_authority(&game), exact_authority(&untouched));
}

#[test]
fn an_unquotable_card_does_not_prevent_the_rest_of_the_observation() {
    let game = board_with(&json!(i64::MAX.to_string()), "unit-spell", true);
    let cost = preview(&game, Viewer::P1).cost;
    assert_eq!(cost.total_pp, None);
    assert_eq!(cost.pending, [PendingCost::Unavailable]);
    assert_eq!(cost.groups[0].lines, [CostLine::Pp(PpQuote::Unavailable)]);
}

pub(super) fn optional(cost: &CostSummary) -> &CostGroupKind {
    &cost
        .groups
        .iter()
        .find(|group| matches!(group.kind, CostGroupKind::Optional { .. }))
        .unwrap()
        .kind
}
