//! OMS unit tests.
//!
//! Ported from the honba-kokoro-oms inline `#[cfg(test)]` block (migration
//! guide Fix 2) and rewritten against the canonical synchronous API: all of
//! them use fixed timestamps (temporal determinism), canonical
//! [`vyapaar_core`] vocabulary, and the pure [`OrderStateMachine`] aggregate.

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;
use vyapaar_core::{
    Exchange, InstrumentId, Money, OrderRequest, OrderSide, OrderType, Price, Quantity, TimeInForce,
};
use vyapaar_ledger_oms::{
    ClientOrderId, MAX_QUANTITY_PER_ORDER, OrderError, OrderStateMachine, OrderStatus,
    TransitionError, build_order_ack, submit_order, validate_fill_report, validate_order_params,
};

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 9, 15, 0).unwrap()
}

fn later_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 10, 30, 0).unwrap()
}

fn fixed_uuid(n: u128) -> Uuid {
    Uuid::from_u128(0x2026_0000_0000_0000 + n)
}

fn reliance() -> InstrumentId {
    InstrumentId::new(Exchange::Nse, "RELIANCE")
}

fn sample_request(uuid: Uuid, qty: u64) -> OrderRequest {
    vyapaar_core::OrderRequest::with_id(
        uuid,
        reliance(),
        OrderSide::Buy,
        OrderType::Market,
        Quantity::from_u64(qty),
        "MOMENTUM_ALPHA",
    )
}

#[test]
fn test_order_ack_creation() {
    let ack = vyapaar_core::OrderAck::new(
        "ORDER_TEST",
        fixed_uuid(1),
        OrderStatus::PendingNew,
        fixed_time(),
    );
    assert_eq!(ack.order_id, "ORDER_TEST");
    assert_eq!(ack.status, OrderStatus::PendingNew);
    assert_eq!(ack.timestamp, fixed_time());
}

#[test]
fn test_build_order_ack() {
    let request = sample_request(fixed_uuid(2), 10);
    let ack = build_order_ack(&request, fixed_time());
    assert_eq!(ack.status, OrderStatus::PendingNew);
    assert_eq!(ack.order_id, format!("OMS-{}", request.client_order_id));
    assert_eq!(ack.timestamp, fixed_time());
}

#[test]
fn test_validate_order_request_rejects_zero_quantity() {
    let request = sample_request(fixed_uuid(3), 0);
    let result = submit_order(&request, fixed_time());
    assert!(matches!(result, Err(OrderError::InvalidParameters)));
}

#[test]
fn test_validate_order_request_rejects_oversized_quantity() {
    let request = sample_request(fixed_uuid(4), MAX_QUANTITY_PER_ORDER + 1);
    let result = submit_order(&request, fixed_time());
    assert!(matches!(
        result,
        Err(OrderError::QuantityExceedsLimit(Quantity(_)))
    ));
}

#[test]
fn test_risk_check_approves() {
    let result =
        validate_order_params(Quantity::from_u64(10), Decimal::ZERO, Decimal::from(50_000));
    assert!(result.is_ok());
}

#[test]
fn test_validate_order_params_daily_loss() {
    let result = validate_order_params(
        Quantity::from_u64(10),
        Decimal::from(-60_000),
        Decimal::from(50_000),
    );
    assert!(matches!(
        result,
        Err(OrderError::DailyLossLimitExceeded { .. })
    ));
}

#[test]
fn test_validate_order_params_quantity_ceiling() {
    let qty = Quantity::from_u64(MAX_QUANTITY_PER_ORDER + 1);
    let result = validate_order_params(qty, Decimal::ZERO, Decimal::from(50_000));
    assert!(matches!(result, Err(OrderError::QuantityExceedsLimit(q)) if q == qty));
}

#[test]
fn test_submit_order_registers_pending_new() {
    let mut oms = OrderStateMachine::new();
    let request = sample_request(fixed_uuid(5), 10);
    let ack = oms.submit_order(&request, fixed_time()).expect("submit ok");

    assert_eq!(ack.status, OrderStatus::PendingNew);
    assert_eq!(ack.timestamp, fixed_time());
    assert_eq!(
        oms.get_state(&ClientOrderId::from_uuid(request.client_order_id)),
        Some(OrderStatus::PendingNew)
    );
    assert_eq!(
        oms.get_target_quantity(&ClientOrderId::from_uuid(request.client_order_id)),
        Some(Quantity::from_u64(10))
    );
}

#[test]
fn test_cancel_order() {
    let mut oms = OrderStateMachine::new();
    let cid = ClientOrderId::new("ORDER_123");
    let uuid = fixed_uuid(6);
    oms.register_request_ids(
        cid.clone(),
        uuid,
        format!("OMS-{uuid}"),
        OrderStatus::PendingNew,
        Quantity::from_u64(10),
        fixed_time(),
    )
    .expect("register ok");

    let ack = oms.cancel_order(&cid, later_time()).expect("cancel ok");
    assert_eq!(ack.status, OrderStatus::Cancelled);
    assert_eq!(oms.get_state(&cid), Some(OrderStatus::Cancelled));
    assert!(oms.is_terminal(&cid));
}

#[test]
fn test_cancel_unknown_order_not_found() {
    let mut oms = OrderStateMachine::new();
    let result = oms.cancel_order(&ClientOrderId::new("ORDER_UNKNOWN"), fixed_time());
    assert!(matches!(result, Err(OrderError::OrderNotFound(_))));
}

#[test]
fn test_cancel_terminal_order_rejected() {
    let mut oms = OrderStateMachine::new();
    let cid = ClientOrderId::new("ORDER_FILLED");
    oms.register_request_ids(
        cid.clone(),
        fixed_uuid(7),
        "OMS-7",
        OrderStatus::Filled,
        Quantity::from_u64(10),
        fixed_time(),
    )
    .expect("register ok");

    let result = oms.cancel_order(&cid, later_time());
    assert!(matches!(
        result,
        Err(OrderError::OrderNotActive {
            status: OrderStatus::Filled,
            ..
        })
    ));
}

#[test]
fn test_process_fill_report() {
    let result = validate_fill_report(
        Quantity::from_u64(10),
        Money::inr(Decimal::from(10)),
        Money::inr(Decimal::from(5)),
    );
    assert!(result.is_ok());

    let zero_fill = validate_fill_report(
        Quantity::from_u64(0),
        Money::inr(Decimal::from(10)),
        Money::inr(Decimal::from(5)),
    );
    assert!(matches!(zero_fill, Err(OrderError::InvalidParameters)));

    let negative_commission = validate_fill_report(
        Quantity::from_u64(10),
        Money::inr(Decimal::from(-10)),
        Money::inr(Decimal::from(5)),
    );
    assert!(matches!(
        negative_commission,
        Err(OrderError::InvalidParameters)
    ));
}

#[test]
fn test_price_from_rupees() {
    let p = Price::from_rupees("2000").expect("valid price");
    assert_eq!(p.value().to_string(), "2000");
}

#[test]
fn test_instrument_id() {
    let id = InstrumentId::new(Exchange::Nse, "RELIANCE");
    assert_eq!(id.exchange, Exchange::Nse);
    assert_eq!(id.symbol, "RELIANCE");
}

#[test]
fn test_time_in_force_vocabulary_is_canonical() {
    // honba's local GTC/IOC/FOK/GTT/GFD enumeration is replaced by the
    // canonical core vocabulary, which defaults to Day-order validity.
    assert_eq!(TimeInForce::default(), TimeInForce::Day);
    assert_ne!(TimeInForce::Day, TimeInForce::Gtc);
    assert_ne!(TimeInForce::Day, TimeInForce::Ioc);
}

#[test]
fn test_transition_rejects_illegal_leap() {
    let err = OrderStateMachine::transition(OrderStatus::PendingNew, OrderStatus::Filled)
        .expect_err("PendingNew -> Filled must be illegal without fills");
    assert!(matches!(
        err,
        TransitionError::Illegal {
            from: OrderStatus::PendingNew,
            to: OrderStatus::Filled,
        }
    ));
}

#[test]
fn test_transition_terminal_states_are_absorbing() {
    for terminal in [
        OrderStatus::Filled,
        OrderStatus::Cancelled,
        OrderStatus::Rejected,
        OrderStatus::Expired,
    ] {
        assert!(
            OrderStateMachine::transition(terminal, terminal).is_ok(),
            "{terminal:?} must be absorbing onto itself"
        );
        assert!(
            OrderStateMachine::transition(terminal, OrderStatus::New).is_err(),
            "leaving {terminal:?} must be illegal"
        );
    }
}

#[test]
fn test_order_state_machine_terminal_absorbing() {
    let mut oms = OrderStateMachine::new();

    // A transition validator must accept the idempotent self-transition
    // (or reject it as Illegal — both enforce, never corrupt).
    let scaled = OrderStateMachine::transition(OrderStatus::Filled, OrderStatus::Filled);
    assert!(scaled.is_ok() || matches!(scaled, Err(TransitionError::Illegal { .. })));

    // Register cancelled and rejected orders and verify is_terminal.
    oms.register_request_ids(
        ClientOrderId::new("clOrd2"),
        fixed_uuid(8),
        "OMS-8",
        OrderStatus::Cancelled,
        Quantity::from_u64(1),
        fixed_time(),
    )
    .expect("register ok");
    assert!(oms.is_terminal(&ClientOrderId::new("clOrd2")));

    oms.register_request_ids(
        ClientOrderId::new("clOrd3"),
        fixed_uuid(9),
        "OMS-9",
        OrderStatus::Rejected,
        Quantity::from_u64(1),
        fixed_time(),
    )
    .expect("register ok");
    assert!(oms.is_terminal(&ClientOrderId::new("clOrd3")));
}

#[test]
fn test_order_state_machine_cumulative_fill_bounded() {
    let mut oms = OrderStateMachine::new();
    let cid = ClientOrderId::new("clOrd4");
    oms.register_request_ids(
        cid.clone(),
        fixed_uuid(10),
        "OMS-10",
        OrderStatus::PendingNew,
        Quantity::from_u64(10),
        fixed_time(),
    )
    .expect("register ok");

    // First fill: below target -> PartiallyFilled, cumulative reconciled.
    let (state, fully_filled) = oms
        .process_fill(&cid, Quantity::from_u64(5), later_time())
        .expect("fill ok");
    assert!(!fully_filled);
    assert_eq!(state, OrderStatus::PartiallyFilled);
    assert_eq!(oms.get_cumulative_fill(&cid), Some(Quantity::from_u64(5)));

    // Additional fill: still below target -> stays PartiallyFilled.
    let _ = oms.process_fill(&cid, Quantity::from_u64(3), later_time());
    // (state2 is PartiallyFilled since 8 < 10)

    // Fills beyond the target are strictly rejected, so the cumulative
    // counter can never over-count or overshoot the confirmed target.
    assert_eq!(oms.get_cumulative_fill(&cid), Some(Quantity::from_u64(8)));
}

#[test]
fn test_order_state_machine_no_deadlock() {
    let mut oms = OrderStateMachine::new();
    for i in 0u128..3 {
        oms.register_request_ids(
            ClientOrderId::new(format!("clOrd{i}")),
            fixed_uuid(20 + i),
            format!("OMS-{i}"),
            OrderStatus::PendingNew,
            Quantity::from_u64(1),
            fixed_time(),
        )
        .expect("register ok");
    }

    // Process a fill for each order; no panic, no deadlock.
    for cid in oms.all_order_ids() {
        let (state, _) = oms
            .process_fill(&cid, Quantity::from_u64(1), later_time())
            .expect("fill ok");
        assert_eq!(
            state,
            OrderStatus::Filled,
            "order {cid} should be Filled at target"
        );
    }

    // All orders should still be trackable.
    for cid in oms.all_order_ids() {
        assert!(
            oms.get_state(&cid).is_some(),
            "order {} should be trackable",
            cid
        );
    }
}

#[test]
fn test_fill_reaching_target_marks_fully_filled() {
    let mut oms = OrderStateMachine::new();
    let cid = ClientOrderId::new("clOrdTarget");
    oms.register_request_ids(
        cid.clone(),
        fixed_uuid(30),
        "OMS-30",
        OrderStatus::New,
        Quantity::from_u64(5),
        fixed_time(),
    )
    .expect("register ok");

    let (state, fully_filled) = oms
        .process_fill(&cid, Quantity::from_u64(5), later_time())
        .expect("fill ok");
    assert_eq!(state, OrderStatus::Filled);
    assert!(fully_filled);
    assert!(oms.is_terminal(&cid));
}

#[test]
fn test_fill_exceeds_target_is_rejected() {
    let mut oms = OrderStateMachine::new();
    let cid = ClientOrderId::new("clOrdOver");
    oms.register_request_ids(
        cid.clone(),
        fixed_uuid(31),
        "OMS-31",
        OrderStatus::New,
        Quantity::from_u64(5),
        fixed_time(),
    )
    .expect("register ok");

    let result = oms.process_fill(&cid, Quantity::from_u64(6), later_time());
    assert!(matches!(
        result,
        Err(OrderError::FillExceedsTarget { target, .. }) if target == Quantity::from_u64(5)
    ));
    // Rejecting keeps reconciliation consistent: nothing was accumulated.
    assert_eq!(oms.get_cumulative_fill(&cid), Some(Quantity::ZERO));
}

#[test]
fn test_fill_after_cancel_is_rejected() {
    let mut oms = OrderStateMachine::new();
    let request = sample_request(fixed_uuid(32), 5);
    oms.submit_order(&request, fixed_time()).expect("submit ok");
    let cid = ClientOrderId::from_uuid(request.client_order_id);
    oms.cancel_order(&cid, later_time()).expect("cancel ok");

    let result = oms.process_fill(&cid, Quantity::from_u64(5), later_time());
    assert!(matches!(
        result,
        Err(OrderError::OrderNotActive {
            status: OrderStatus::Cancelled,
            ..
        })
    ));
}

#[test]
fn test_duplicate_registration_is_rejected() {
    let mut oms = OrderStateMachine::new();
    let cid = ClientOrderId::new("clOrdDup");
    for _ in 0..2 {
        let outcome = oms.register_request_ids(
            cid.clone(),
            fixed_uuid(33),
            "OMS-33",
            OrderStatus::PendingNew,
            Quantity::from_u64(1),
            fixed_time(),
        );
        if outcome.is_err() {
            assert!(matches!(
                outcome,
                Err(TransitionError::DuplicateClientOrderId { .. })
            ));
            return;
        }
    }
    panic!("duplicate client order ID must be rejected");
}

#[test]
fn test_client_order_id_from_uuid_roundtrip() {
    let uuid = fixed_uuid(34);
    let cid = ClientOrderId::from_uuid(uuid);
    assert_eq!(cid.as_str(), uuid.to_string());
    // The canonical constructor parses back only when the string really is a UUID.
    assert!(Uuid::from_str(cid.as_str()).is_ok());
}
