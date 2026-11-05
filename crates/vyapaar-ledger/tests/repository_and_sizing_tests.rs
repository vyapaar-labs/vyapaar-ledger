//! Pure domain repository traits & in-memory repositories, and the risk-aware
//! position sizing service hook (migration guide Fix 5).
//!
//! Ported from `honba-kokoro/tests/repository_and_sizing_tests.rs` with the
//! Tier 1B determinism fixes:
//! - No `Utc::now()` anywhere: every timestamp is a fixed instant (the FIFO
//!   fill lands at 2026-02-20 09:15 UTC so the 2025-01-10 entry is 406 days
//!   old — long-term by construction, not by "run the test in January").
//! - The canonical core [`FillReport`] carries no account field and mints no
//!   UUIDs: the trading account is an explicit argument to
//!   `PositionRepository::match_lots`, and `client_order_id` is a caller-
//!   supplied fixed `Uuid::from_u128`.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;

use vyapaar_core::sizing::{PositionSizingRequest, SizingMethod, calculate_position_size};
use vyapaar_core::uuid::Uuid;
use vyapaar_core::{
    Currency, Exchange, InstrumentId, Money, OrderSide, OrderStatus, OrderType, PositionSide,
    Price, Quantity, TimeInForce,
};
use vyapaar_ledger::portfolio::{AccountState, HeldPosition, PortfolioState};
use vyapaar_ledger::{
    AccountCode, AccountId, AlgoId, ClientOrderId, DomainError, FillReport,
    InMemoryLedgerRepository, InMemoryOrderRepository, InMemoryPositionRepository,
    JournalTransaction, LedgerRepository, OrderRecord, OrderRepository, Portfolio,
    PositionRepository, PostingEntry, TaxLot, TransactionId, evaluate_risk_budget,
};

/// The fixed execution instant every test shares: 2026-02-20 09:15 UTC.
/// The domain never mints a timestamp; the test injects this one.
fn fixed_fill_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 2, 20, 9, 15, 0).unwrap()
}

#[test]
fn test_order_repository_lifecycle() {
    let mut repo = InMemoryOrderRepository::new();
    let client_id = ClientOrderId::new("ORD_TEST_001");
    let algo_1 = AlgoId::new("MOMENTUM_ALPHA");
    let algo_2 = AlgoId::new("MEAN_REVERSION");
    let account = AccountId::new("PROP_DESK_1");
    let instrument = InstrumentId::new(Exchange::Nse, "RELIANCE");

    let now = fixed_fill_time();
    let order = OrderRecord {
        client_order_id: client_id.clone(),
        broker_order_id: Some("BROKER_99182".to_string()),
        algo_id: Some(algo_1.clone()),
        account_id: account.clone(),
        instrument: instrument.clone(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit(Price::from_rupees("2500.00").unwrap()),
        time_in_force: TimeInForce::Day,
        price: Some(Price::from_rupees("2500.00").unwrap()),
        quantity: Quantity::from_u64(100),
        filled_quantity: Quantity::from_u64(25),
        avg_fill_price: Some(Price::from_rupees("2498.50").unwrap()),
        status: OrderStatus::PartiallyFilled,
        created_at: now,
        updated_at: now,
    };

    // Save order
    assert!(repo.save_order(&order).is_ok());

    // Retrieve order
    let fetched = repo.get_order(&client_id).unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.client_order_id, client_id);
    assert_eq!(fetched.status, OrderStatus::PartiallyFilled);
    assert_eq!(fetched.quantity, Quantity::from_u64(100));

    // Active orders query by algo_id
    let active_algo_1 = repo.list_active_orders(&algo_1).unwrap();
    assert_eq!(active_algo_1.len(), 1);
    assert_eq!(active_algo_1[0].client_order_id, client_id);

    let active_algo_2 = repo.list_active_orders(&algo_2).unwrap();
    assert_eq!(active_algo_2.len(), 0);

    // Terminal order transition (Filled)
    let mut filled_order = order.clone();
    filled_order.status = OrderStatus::Filled;
    filled_order.filled_quantity = Quantity::from_u64(100);
    assert!(repo.save_order(&filled_order).is_ok());

    // Active orders should now be empty for algo_1
    let active_algo_1_after = repo.list_active_orders(&algo_1).unwrap();
    assert_eq!(active_algo_1_after.len(), 0);
}

#[test]
fn test_position_repository_fifo_tax_lot_matching() {
    let mut repo = InMemoryPositionRepository::new();
    let account = AccountId::new("CLIENT_ACCOUNT_42");
    let instrument = InstrumentId::new(Exchange::Nse, "TCS");

    // Fixed fill instant: 2026-02-20 09:15 UTC. Pinned so the holding-period
    // assertions below hold by arithmetic, not by the wall clock.
    let fill_date = fixed_fill_time();

    // Lot 1: 50 shares @ ₹3,000 on 2025-01-10 (406 days before the fill → Long-Term)
    let lot_1 = TaxLot {
        lot_id: "LOT_001".to_string(),
        account_id: account.clone(),
        instrument: instrument.clone(),
        side: PositionSide::Long,
        entry_price: Price::from_rupees("3000.00").unwrap(),
        quantity: Quantity::from_u64(50),
        remaining_quantity: Quantity::from_u64(50),
        entry_date: NaiveDate::from_ymd_opt(2025, 1, 10).unwrap(),
        created_at: Utc.with_ymd_and_hms(2025, 1, 10, 9, 15, 0).unwrap(),
    };

    // Lot 2: 50 shares @ ₹3,200 on 2025-06-15 (250 days before the fill → Short-Term)
    let lot_2 = TaxLot {
        lot_id: "LOT_002".to_string(),
        account_id: account.clone(),
        instrument: instrument.clone(),
        side: PositionSide::Long,
        entry_price: Price::from_rupees("3200.00").unwrap(),
        quantity: Quantity::from_u64(50),
        remaining_quantity: Quantity::from_u64(50),
        entry_date: NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        created_at: Utc.with_ymd_and_hms(2025, 6, 15, 9, 15, 0).unwrap(),
    };

    assert!(repo.save_lot(&account, &lot_1).is_ok());
    assert!(repo.save_lot(&account, &lot_2).is_ok());

    let positions = repo.get_positions(&account).unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].quantity, Quantity::from_u64(100));

    // Fill report: Sell 70 shares @ ₹3,500
    // FIFO expects:
    // - 50 shares from LOT_001: PnL = 50 * (3500 - 3000) = ₹25,000 (Long-Term)
    // - 20 shares from LOT_002: PnL = 20 * (3500 - 3200) = ₹6,000 (Short-Term)
    // - 30 shares remaining in LOT_002
    //
    // The canonical core FillReport carries its own account id, so `match_lots` reads the
    // account from the fill rather than taking it separately.
    let sell_fill = FillReport::new(
        "TRD_SELL_901",
        "ORD_SELL_01",
        Uuid::from_u128(0x2026_0000_0001),
        instrument.clone(),
        OrderSide::Sell,
        Price::from_rupees("3500.00").unwrap(),
        Quantity::from_u64(70),
        "MOMENTUM_ALPHA",
        fill_date,
        account.as_str(),
    )
    .with_fee(Money::inr(Decimal::from(50)));

    let matched = repo.match_lots(&sell_fill).unwrap();
    assert_eq!(matched.len(), 2);

    // First matched lot (LOT_001)
    assert_eq!(matched[0].lot_id, "LOT_001");
    assert_eq!(matched[0].matched_quantity, Quantity::from_u64(50));
    assert_eq!(
        matched[0].entry_price,
        Price::from_rupees("3000.00").unwrap()
    );
    assert_eq!(matched[0].realized_pnl.amount, Decimal::from(25_000));
    assert!(matched[0].is_long_term);

    // Second matched lot (LOT_002)
    assert_eq!(matched[1].lot_id, "LOT_002");
    assert_eq!(matched[1].matched_quantity, Quantity::from_u64(20));
    assert_eq!(
        matched[1].entry_price,
        Price::from_rupees("3200.00").unwrap()
    );
    assert_eq!(matched[1].realized_pnl.amount, Decimal::from(6_000));

    // Position aggregate after matching
    let positions_after = repo.get_positions(&account).unwrap();
    assert_eq!(positions_after[0].quantity, Quantity::from_u64(30));
    assert_eq!(
        positions_after[0].realized_pnl.amount,
        Decimal::from(31_000) // 25000 + 6000
    );
}

#[test]
fn test_ledger_repository_posting_and_balance() {
    let mut repo = InMemoryLedgerRepository::new(Currency::Inr);
    let cash_acct = AccountCode::new(AccountCode::CASH_AT_BROKER);
    let capital_acct = AccountCode::new(AccountCode::OPENING_CAPITAL);

    // 1. Initial Deposit: Debit Cash ₹10,00,000, Credit Capital ₹10,00,000
    let tx1 = JournalTransaction::new(
        "TX_DEPOSIT_001",
        fixed_fill_time(),
        "Capital funding",
        vec![PostingEntry {
            account: cash_acct.clone(),
            amount: Money::inr(Decimal::from(1_000_000)),
        }],
        vec![PostingEntry {
            account: capital_acct.clone(),
            amount: Money::inr(Decimal::from(1_000_000)),
        }],
    )
    .unwrap();

    let tx_id = repo.post_entry(&tx1).unwrap();
    assert_eq!(tx_id, TransactionId::new("TX_DEPOSIT_001"));

    let cash_balance = repo.get_balance(&cash_acct).unwrap();
    assert_eq!(cash_balance.amount, Decimal::from(1_000_000));

    // 2. Unbalanced entry must be rejected
    let tx_unbalanced = JournalTransaction {
        id: TransactionId::new("TX_BAD"),
        timestamp: fixed_fill_time(),
        description: "Bad transaction".to_string(),
        debits: vec![PostingEntry {
            account: cash_acct.clone(),
            amount: Money::inr(Decimal::from(500)),
        }],
        credits: vec![PostingEntry {
            account: capital_acct.clone(),
            amount: Money::inr(Decimal::from(200)),
        }],
    };
    let post_bad = repo.post_entry(&tx_unbalanced);
    assert!(matches!(post_bad, Err(DomainError::UnbalancedLedger(..))));
}

#[test]
fn test_evaluate_risk_budget_and_sizing_integration() {
    let instrument = InstrumentId::new(Exchange::Nse, "INFY");
    let mut state = PortfolioState::new(Currency::Inr);

    let mut acc = AccountState::new(Currency::Inr, vec![]);
    acc.cash = Money::inr(Decimal::from(500_000)); // ₹5,00,000 free cash

    // Existing position: 100 shares @ ₹1,500 = ₹1,50,000 notional
    let pos = HeldPosition::new(
        instrument.clone(),
        PositionSide::Long,
        Quantity::from_u64(100),
        Price::from_rupees("1500.00").unwrap(),
    );
    acc.positions.push(pos);

    state.accounts.insert("MAIN_ACCOUNT".to_string(), acc);

    // Total equity = 500,000 + 150,000 = 650,000
    // Gross exposure = 150,000
    // Leverage limit = 4.0x
    // Scrip concentration = 25% of 650,000 = ₹1,62,500
    let portfolio = Portfolio::with_limits(
        state,
        Decimal::from(4),
        Decimal::new(25, 2), // 25%
    );

    let risk_pct = Decimal::new(1, 2); // 1% risk per trade = 0.01
    let budget = evaluate_risk_budget(&portfolio, risk_pct);

    assert_eq!(budget.equity.amount, Decimal::from(500_000)); // free cash
    assert_eq!(budget.risk_pct, risk_pct);

    let max_notional = budget.max_position_notional.unwrap();
    // 25% of ₹6,50,000 = ₹1,62,500
    assert_eq!(max_notional.amount, Decimal::from(162_500));

    // Dynamic sizing calculation integration with vyapaar-core::sizing
    let sizing_request = PositionSizingRequest::new(
        instrument.clone(),
        PositionSide::Long,
        Price::from_rupees("1500.00").unwrap(),
        budget.equity,
        budget.clone(),
        SizingMethod::FixedFractional { risk_pct },
    )
    .with_stop_loss(Price::from_rupees("1470.00").unwrap()); // ₹30 risk per share

    let sizing_result = calculate_position_size(&sizing_request).unwrap();

    // 1% risk of ₹5,00,000 = ₹5,000
    // Risk per share = ₹30
    // Raw qty = 5000 / 30 = 166.66 -> 166 shares
    // Notional = 166 * 1500 = ₹2,49,000
    // But max_notional = ₹1,62,500
    // Allowed shares = floor(162,500 / 1500) = 108 shares
    assert_eq!(sizing_result.quantity, Quantity::from_u64(108));
    assert!(sizing_result.capped_by_limit);
    assert_eq!(
        sizing_result.allocated_notional.amount,
        Decimal::from(162_000) // 108 * 1500
    );
}
