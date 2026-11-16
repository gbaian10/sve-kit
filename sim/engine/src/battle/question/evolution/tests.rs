use super::*;

const fn quote(
    total_pp: i64,
    available_pp: i64,
    ep_allowed: bool,
    super_allowed: bool,
) -> player::EvolutionPayment {
    player::EvolutionPayment {
        total_pp,
        available_pp,
        ep_allowed,
        super_allowed,
    }
}

fn offered(domain: player::EvolutionPayment) -> Result<Vec<wire::EvolutionPayment>> {
    let (_, shape, _) = evolution_payment(domain)?;
    let wire::QuestionShape::EvolutionPayment { payments } = shape else {
        return Err(ProjectionError::Unavailable);
    };
    Ok(payments)
}

#[test]
fn evolution_payment_projection_rejects_negative_quotes_and_empty_domains() {
    for domain in [
        quote(-1, 1, false, false),
        quote(1, -1, true, true),
        quote(2, 0, true, true),
        quote(1, 0, false, false),
    ] {
        assert_eq!(
            offered(domain),
            Err(ProjectionError::Unavailable),
            "an invalid or empty domain cannot produce an answerable question"
        );
    }
}

#[test]
fn evolution_payment_projection_checks_the_actual_encoded_pp_range() {
    let maximum = Int::MAX.get();
    let first_unsafe = maximum.checked_add(1).unwrap();
    assert!(
        !quote(i64::MAX, i64::MAX, true, true).is_valid(i64::MAX, true, false),
        "EP addition cannot overflow into a valid native payment"
    );
    assert_eq!(
        offered(quote(maximum, maximum, false, false)),
        Ok(vec![wire::EvolutionPayment {
            pp: Int::MAX,
            use_ep: false,
            super_evolution: false,
        }]),
        "the largest wire PP remains representable"
    );
    for domain in [
        quote(first_unsafe, first_unsafe, false, false),
        quote(first_unsafe, first_unsafe, true, true),
        quote(i64::MAX, i64::MAX, true, true),
    ] {
        assert_eq!(
            offered(domain),
            Err(ProjectionError::Range),
            "one unencodable legal tuple refuses the entire question"
        );
    }
}

#[test]
fn evolution_payment_projection_filters_native_candidates_before_encoding() {
    let maximum = Int::MAX.get();
    let domain = quote(maximum.checked_add(1).unwrap(), maximum, true, true);
    assert_eq!(
        offered(domain),
        Ok(vec![
            wire::EvolutionPayment {
                pp: Int::MAX,
                use_ep: true,
                super_evolution: false
            },
            wire::EvolutionPayment {
                pp: Int::MAX,
                use_ep: true,
                super_evolution: true
            },
        ]),
        "unaffordable PP is not encoded; EP can make the remaining tuples representable"
    );
    assert_eq!(
        offered(quote(0, 0, true, true)),
        Ok(vec![
            wire::EvolutionPayment {
                pp: Int::ZERO,
                use_ep: false,
                super_evolution: false
            },
            wire::EvolutionPayment {
                pp: Int::ZERO,
                use_ep: false,
                super_evolution: true
            },
        ]),
        "a zero total cannot spend EP even when the input permits considering it"
    );
}

#[test]
fn evolution_payment_projection_matches_native_legal_domain_for_small_quotes() {
    for total in 0_i64..=3_i64 {
        for available in 0_i64..=3_i64 {
            for ep_allowed in [false, true] {
                for super_allowed in [false, true] {
                    let domain = quote(total, available, ep_allowed, super_allowed);
                    let mut expected = Vec::new();
                    for use_ep in [false, true] {
                        for super_evolution in [false, true] {
                            for pp in 0_i32..=3_i32 {
                                if domain.is_valid(i64::from(pp), use_ep, super_evolution) {
                                    expected.push(wire::EvolutionPayment {
                                        pp: Int::from(pp),
                                        use_ep,
                                        super_evolution,
                                    });
                                }
                            }
                        }
                    }
                    if expected.is_empty() {
                        assert_eq!(
                            offered(domain),
                            Err(ProjectionError::Unavailable),
                            "an empty native legal set must stop projection"
                        );
                    } else {
                        assert_eq!(
                            offered(domain),
                            Ok(expected),
                            "the independent PP sweep must find exactly the native legal domain"
                        );
                    }
                }
            }
        }
    }
}
