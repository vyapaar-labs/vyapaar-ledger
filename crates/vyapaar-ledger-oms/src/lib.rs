//! # व्यापार・閻 vyapaar-ledger-oms
//!
//! Synchronous, deterministic order-management state machine for Indian
//! equities. The engine never places an order directly: it validates, prices,
//! and forwards order requests to brokers (ADR 0032 §7). Here the OMS owns the
//! SEBI order lifecycle: idempotency by client order ID, strict legal state
//! transitions, and fill reconciliation against the confirmed target quantity.
//!
//! # Migration notes (Tier 1B, `honba-kokoro-oms` → `vyapaar-ledger-oms`)
//!
//! - Every duplicated type (`Exchange`, `InstrumentId`, `OrderSide::{Long,Short}`,
//!   `OrderType` flags, `TimeInForce::{GTC,IOC,FOK,GTT,GFD}`, `Currency`, `Money`,
//!   `Quantity(u64)`, `Price`, `OrderStatus::{Pending,Received,Partial}`) was
//!   deleted; the canonical [`vyapaar_core`] vocabulary is used exclusively.
//! - Submission and cancellation are pure synchronous state transitions, not
//!   `async` functions. Time is injected: every entry point takes an explicit
//!   `now: DateTime<Utc>`, and IDs are never minted with `Uuid::new_v4()`
//!   inside the domain (the caller-supplied request carries the correlation ID).
//! - `OrderState::PendingCancel` was dropped: the canonical
//!   [`OrderStatus`] vocabulary has no pre-cancellation state; cancellation
//!   transitions directly to `Cancelled` (SEBI audit streams carry cancellation
//!   *requests* as events, not order states).
//! - The obsolete OMS-local `CostBasisLot` was deleted; tax-lot accounting is
//!   owned by `vyapaar-ledger-portfolio` (`TaxLot` / `AccountState::match_lots`).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;
use vyapaar_core::{Money, OrderAck, OrderRequest, Quantity};

// ——————————————————————————————————————————————————————————————————————————
// Canonical vocabulary (re-exported so downstream gates import only this crate)
// ——————————————————————————————————————————————————————————————————————————

pub use vyapaar_core::{
    ClientOrderId, Currency, Exchange, FillReport, InstrumentId, OrderSide, OrderStatus, OrderType,
    Price, TimeInForce,
};

/// Hard per-order quantity ceiling (pre-trade sanity limit, portfolio-agnostic).
pub const MAX_QUANTITY_PER_ORDER: u64 = 1_000_000;

// ———————————————————————————————————————————————————————————————————
// Errors
// ———————————————————————————————————————————————————————————————————

/// Comprehensive order-layer error enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrderError {
    /// Request quantity exceeds [`MAX_QUANTITY_PER_ORDER`].
    #[error("quantity {0} exceeds the per-order limit of {MAX_QUANTITY_PER_ORDER}")]
    QuantityExceedsLimit(Quantity),
    /// The daily realized loss breached the configured maximum.
    #[error("daily loss limit exceeded: today_pnl {today_pnl} < -{max_loss}")]
    DailyLossLimitExceeded {
        /// Realized PnL accumulated today.
        today_pnl: Decimal,
        /// Configured absolute daily loss ceiling.
        max_loss: Decimal,
    },
    /// Validation rejected the request (zero quantity, negative commission, ...).
    #[error("invalid parameters")]
    InvalidParameters,
    /// The referenced client order ID is unknown to the state machine.
    #[error("order not found: {0}")]
    OrderNotFound(ClientOrderId),
    /// The referenced order is in a terminal state and cannot receive fills or cancellations.
    #[error("order {order_id} is not active (status {status:?})")]
    OrderNotActive {
        /// The client order ID that was addressed.
        order_id: ClientOrderId,
        /// The terminal state it reached.
        status: OrderStatus,
    },
    /// A fill would push cumulative filled quantity beyond the confirmed target.
    #[error("fill for {order_id} would exceed target {target} (projected {filled})")]
    FillExceedsTarget {
        /// The client order ID the fill belongs to.
        order_id: ClientOrderId,
        /// Confirmed target quantity.
        target: Quantity,
        /// The quantity that would have been cumulative after this fill.
        filled: Quantity,
    },
    /// The client order ID is already registered (idempotency violation).
    #[error("duplicate client order ID: {0}")]
    DuplicateClientOrderId(String),
    /// Broker / venue plumbing failure surfaced through the OMS.
    #[error("broker error: {0}")]
    BrokerError(String),
}

/// Attempted an illegal order-state transition.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionError {
    /// The transition `(from -> to)` is not part of the legal SEBI lifecycle graph.
    #[error("illegal transition {from:?} -> {to:?}")]
    Illegal {
        /// Source state.
        from: OrderStatus,
        /// Target state.
        to: OrderStatus,
    },
    /// The client order ID is already registered.
    #[error("duplicate client order ID: {client_order_id}")]
    DuplicateClientOrderId {
        /// The offending client order ID.
        client_order_id: String,
    },
}

// ———————————————————————————————————————————————————————————————————
// OMS context types (canvas the risk gate and broker adapters fill in)
// ———————————————————————————————————————————————————————————————————

/// Market data snapshot used for price-collar validation around submission.
#[derive(Debug, Clone)]
pub struct MarketData {
    /// Instrument the quotes belong to.
    pub instrument: InstrumentId,
    /// Best bid.
    pub bid: Price,
    /// Best ask.
    pub ask: Price,
    /// Last traded price.
    pub last: Price,
    /// Volume accumulated for the day so far.
    pub volume: Quantity,
    /// Quote timestamp, supplied by the caller (never minted here).
    pub timestamp: DateTime<Utc>,
}

/// OMS configuration knobs.
#[derive(Debug, Clone)]
pub struct OmsConfig {
    /// Maximum notional value per single order, if configured.
    pub max_notional_per_order: Option<Decimal>,
    /// Maximum quantity per single order, if configured (falls back to
    /// [`MAX_QUANTITY_PER_ORDER`]).
    pub max_quantity_per_order: Option<Quantity>,
    /// Maximum absolute daily realized loss, if configured.
    pub max_daily_loss: Option<Money>,
    /// Price-collar half-width in basis points, if configured.
    pub price_collar_bps: Option<u32>,
    /// Time in force applied when a request omits one.
    pub default_time_in_force: TimeInForce,
}

/// Point-in-time snapshot of OMS-wide counters.
#[derive(Debug, Clone)]
pub struct OmsSnapshot {
    /// Orders registered so far.
    pub total_orders: u64,
    /// Orders that reached `Filled`.
    pub total_filled: u64,
    /// Orders that reached `Cancelled`.
    pub total_cancelled: u64,
    /// Commission charged so far.
    pub total_commission: Money,
    /// Statutory/other fees charged so far.
    pub total_fees: Money,
    /// Realized PnL today.
    pub today_pnl: Money,
}

/// Portfolio state consumed by pre-submission risk checks.
#[derive(Debug, Clone)]
pub struct PortfolioRiskState {
    /// Free cash available for new orders.
    pub cash: Money,
    /// Net quantity held per instrument.
    pub positions: BTreeMap<InstrumentId, Quantity>,
    /// Realized PnL accumulated today.
    pub today_pnl: Money,
    /// Absolute daily loss limit.
    pub max_daily_loss: Money,
    /// Price-collar half-width in basis points, if configured.
    pub price_collar_bps: Option<u32>,
}

// ———————————————————————————————————————————————————————————————————
// Order lifecycle aggregate
// ———————————————————————————————————————————————————————————————————

/// One tracked order inside [`OrderStateMachine`].
///
/// The entry carries *both* identifier spellings of the same order: the
/// canonical string key ([`ClientOrderId`]) and the correlation UUID embedded
/// in the wire request, so acknowledgements can be built without re-parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderEntry {
    /// Canonical client order identifier (state-machine key).
    pub client_order_id: ClientOrderId,
    /// Correlation UUID carried by the originating [`OrderRequest`].
    pub client_order_uuid: Uuid,
    /// Platform-assigned order identifier (deterministic, derived from the request).
    pub order_id: String,
    /// Current lifecycle state.
    pub state: OrderStatus,
    /// Confirmed target quantity from the submitted request.
    pub target_quantity: Quantity,
    /// Cumulative filled quantity (never exceeds [`OrderEntry::target_quantity`]).
    pub filled_quantity: Quantity,
    /// Creation timestamp injected by the caller.
    pub created_at: DateTime<Utc>,
    /// Last transition timestamp injected by the caller.
    pub updated_at: DateTime<Utc>,
}

impl OrderEntry {
    /// Convenience for constructing a fresh (unfilled) entry.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client_order_id: ClientOrderId,
        client_order_uuid: Uuid,
        order_id: impl Into<String>,
        initial_state: OrderStatus,
        target_quantity: Quantity,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            client_order_id,
            client_order_uuid,
            order_id: order_id.into(),
            state: initial_state,
            target_quantity,
            filled_quantity: Quantity::ZERO,
            created_at,
            updated_at: created_at,
        }
    }

    /// Quantity still pending execution.
    #[must_use]
    pub fn remaining_quantity(&self) -> Quantity {
        Quantity::new(
            (self.target_quantity.value() - self.filled_quantity.value()).max(Decimal::ZERO),
        )
    }

    /// True once the cumulative fill reaches the target quantity.
    #[must_use]
    pub fn is_fully_filled(&self) -> bool {
        self.state == OrderStatus::Filled
    }

    /// True if no further fills or cancellations are applicable.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }
}

/// Order state machine managing the SEBI order lifecycle with idempotency and
/// fill reconciliation.
///
/// - Keys are canonical [`ClientOrderId`]s (idempotency guaranteed per order).
/// - [`OrderStateMachine::process_fill`] transitions to `PartiallyFilled` while
///   cumulative fills remain below the target and to `Filled` once the target
///   is reached; fills beyond the target are strictly rejected so the
///   cumulative counter can never be double-counted.
/// - Terminal states are absorbing: no transition out is legal.
#[derive(Debug, Clone)]
pub struct OrderStateMachine {
    orders: BTreeMap<ClientOrderId, OrderEntry>,
}

impl Default for OrderStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderStateMachine {
    /// Create a new order state machine.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            orders: BTreeMap::new(),
        }
    }

    /// Validate the transition `from -> to` against the legal lifecycle graph.
    ///
    /// Returns `Ok(to)` if legal, `Err(TransitionError::Illegal { from, to })`
    /// otherwise. Terminal states are absorbing (`Filled|Cancelled|Rejected|Expired
    /// -> itself` is legal and idempotent; anything else out of them is illegal).
    ///
    /// The `Result` must be handled: ignoring a rejection would silently
    /// corrupt the lifecycle.
    pub fn transition(from: OrderStatus, to: OrderStatus) -> Result<OrderStatus, TransitionError> {
        use OrderStatus::*;
        let ok = matches!(
            (from, to),
            (PendingNew, New)
                | (PendingNew, Rejected)
                | (PendingNew, Cancelled)
                // A first fill may land before the acknowledgement is processed.
                | (PendingNew, PartiallyFilled)
                | (New, PartiallyFilled)
                | (New, Filled)
                | (New, Cancelled)
                | (New, Rejected)
                | (PartiallyFilled, Filled)
                | (PartiallyFilled, Cancelled)
                // Terminal states are absorbing.
                | (Filled, Filled)
                | (Cancelled, Cancelled)
                | (Rejected, Rejected)
                | (Expired, Expired)
        );
        if ok {
            Ok(to)
        } else {
            Err(TransitionError::Illegal { from, to })
        }
    }

    /// Check idempotency: if the client order ID already exists, return error.
    /// If new, return `Ok`.
    ///
    /// # Errors
    /// [`TransitionError::DuplicateClientOrderId`] when the ID is registered.
    pub fn check_idempotency(
        &self,
        client_order_id: &ClientOrderId,
    ) -> Result<(), TransitionError> {
        if self.orders.contains_key(client_order_id) {
            Err(TransitionError::DuplicateClientOrderId {
                client_order_id: client_order_id.0.clone(),
            })
        } else {
            Ok(())
        }
    }

    /// Register a fully-formed order entry.
    ///
    /// # Errors
    /// [`TransitionError::DuplicateClientOrderId`] when the key already exists.
    pub fn register_order(&mut self, entry: OrderEntry) -> Result<(), TransitionError> {
        self.check_idempotency(&entry.client_order_id)?;
        self.orders.insert(entry.client_order_id.clone(), entry);
        Ok(())
    }

    /// Register a new order with a validated state, target quantity, and
    /// correlation identifiers.
    ///
    /// Thin convenience over [`OrderStateMachine::register_order`] for call
    /// sites that hold the request identifiers directly.
    ///
    /// # Errors
    /// [`TransitionError::DuplicateClientOrderId`] when the key already exists.
    #[allow(clippy::too_many_arguments)]
    pub fn register_request_ids(
        &mut self,
        client_order_id: ClientOrderId,
        client_order_uuid: Uuid,
        order_id: impl Into<String>,
        initial_state: OrderStatus,
        target_quantity: Quantity,
        created_at: DateTime<Utc>,
    ) -> Result<(), TransitionError> {
        let entry = OrderEntry::new(
            client_order_id,
            client_order_uuid,
            order_id,
            initial_state,
            target_quantity,
            created_at,
        );
        self.register_order(entry)
    }

    /// Validate and register an order request, returning its acknowledgement.
    ///
    /// Pure synchronous state transition: no I/O, no clock, no UUID minting.
    /// The order ID is derived deterministically from the request correlation
    /// UUID; time comes from the `now` parameter.
    ///
    /// # Errors
    /// [`OrderError`]: quantity/validation rejection or duplicate registration.
    pub fn submit_order(
        &mut self,
        request: &OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, OrderError> {
        validate_order_request(request)?;
        let client_order_uuid = request.client_order_id;
        let client_order_id = ClientOrderId::from_uuid(client_order_uuid);
        let order_id = platform_order_id(client_order_uuid);
        self.register_request_ids(
            client_order_id,
            client_order_uuid,
            order_id.clone(),
            OrderStatus::PendingNew,
            request.quantity,
            now,
        )
        .map_err(|e| match e {
            TransitionError::DuplicateClientOrderId { client_order_id } => {
                OrderError::DuplicateClientOrderId(client_order_id)
            }
            other => OrderError::BrokerError(other.to_string()),
        })?;
        Ok(build_order_ack(request, now))
    }

    /// Cancel an active order. Terminal states are absorbing, so cancelling a
    /// `Filled` / `Rejected` / `Expired` order is invalid.
    ///
    /// # Errors
    /// [`OrderError::OrderNotFound`] when unknown,
    /// [`OrderError::OrderNotActive`] when already terminal.
    pub fn cancel_order(
        &mut self,
        client_order_id: &ClientOrderId,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, OrderError> {
        let entry = self
            .orders
            .get_mut(client_order_id)
            .ok_or_else(|| OrderError::OrderNotFound(client_order_id.clone()))?;
        if entry.is_terminal() {
            return Err(OrderError::OrderNotActive {
                order_id: client_order_id.clone(),
                status: entry.state,
            });
        }
        let from = entry.state;
        OrderStateMachine::transition(from, OrderStatus::Cancelled).map_err(|_| {
            OrderError::BrokerError(format!("illegal transition {from:?} -> Cancelled"))
        })?;
        entry.state = OrderStatus::Cancelled;
        entry.updated_at = now;
        Ok(OrderAck::new(
            entry.order_id.clone(),
            entry.client_order_uuid,
            OrderStatus::Cancelled,
            now,
        ))
    }

    /// Process a fill event: reconcile against the target quantity, transition
    /// the state, and return `(state, fully_filled)`.
    ///
    /// `fully_filled` is true exactly when the cumulative fill reaches the
    /// confirmed target. Fills that would overshoot the target are rejected
    /// (never silently saturated) so reconciliation stays auditable.
    ///
    /// # Errors
    /// [`OrderError::OrderNotFound`], [`OrderError::OrderNotActive`],
    /// [`OrderError::InvalidParameters`] (non-positive fill),
    /// [`OrderError::FillExceedsTarget`].
    pub fn process_fill(
        &mut self,
        client_order_id: &ClientOrderId,
        fill_quantity: Quantity,
        now: DateTime<Utc>,
    ) -> Result<(OrderStatus, bool), OrderError> {
        let entry = self
            .orders
            .get_mut(client_order_id)
            .ok_or_else(|| OrderError::OrderNotFound(client_order_id.clone()))?;
        if entry.is_terminal() {
            return Err(OrderError::OrderNotActive {
                order_id: client_order_id.clone(),
                status: entry.state,
            });
        }
        if fill_quantity.value() <= Decimal::ZERO {
            return Err(OrderError::InvalidParameters);
        }
        let projected = entry.filled_quantity.value() + fill_quantity.value();
        let target = entry.target_quantity.value();
        if projected > target {
            return Err(OrderError::FillExceedsTarget {
                order_id: client_order_id.clone(),
                target: entry.target_quantity,
                filled: Quantity::new(projected),
            });
        }

        entry.filled_quantity = Quantity::new(projected);
        entry.updated_at = now;
        let state = if projected >= target {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        entry.state = state;
        Ok((state, state == OrderStatus::Filled))
    }

    /// Get the current state of an order.
    #[must_use]
    pub fn get_state(&self, client_order_id: &ClientOrderId) -> Option<OrderStatus> {
        self.orders.get(client_order_id).map(|e| e.state)
    }

    /// Get the cumulative filled quantity for an order.
    #[must_use]
    pub fn get_cumulative_fill(&self, client_order_id: &ClientOrderId) -> Option<Quantity> {
        self.orders.get(client_order_id).map(|e| e.filled_quantity)
    }

    /// Get the confirmed target quantity for an order.
    #[must_use]
    pub fn get_target_quantity(&self, client_order_id: &ClientOrderId) -> Option<Quantity> {
        self.orders.get(client_order_id).map(|e| e.target_quantity)
    }

    /// Check if an order is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self, client_order_id: &ClientOrderId) -> bool {
        self.orders
            .get(client_order_id)
            .is_some_and(OrderEntry::is_terminal)
    }

    /// Get all client order IDs, ordered (BTreeMap iteration).
    #[must_use]
    pub fn all_order_ids(&self) -> Vec<ClientOrderId> {
        self.orders.keys().cloned().collect()
    }
}

// ———————————————————————————————————————————————————————————————————
// Pure validation and acknowledgement builders
// ———————————————————————————————————————————————————————————————————

/// Validate an order request against OMS sanity limits.
///
/// # Errors
/// - [`OrderError::InvalidParameters`] for a zero quantity.
/// - [`OrderError::QuantityExceedsLimit`] above [`MAX_QUANTITY_PER_ORDER`].
pub fn validate_order_request(request: &OrderRequest) -> Result<(), OrderError> {
    let qty = request.quantity.value();
    if qty <= Decimal::ZERO {
        return Err(OrderError::InvalidParameters);
    }
    let max = Decimal::from(MAX_QUANTITY_PER_ORDER);
    if qty > max {
        return Err(OrderError::QuantityExceedsLimit(request.quantity));
    }
    Ok(())
}

/// Validate order parameters against the per-order ceiling and the daily loss limit.
///
/// # Errors
/// - [`OrderError::QuantityExceedsLimit`] above [`MAX_QUANTITY_PER_ORDER`].
/// - [`OrderError::DailyLossLimitExceeded`] when `today_pnl < -max_loss`.
pub fn validate_order_params(
    qty: Quantity,
    today_pnl: Decimal,
    max_loss: Decimal,
) -> Result<(), OrderError> {
    let max = Decimal::from(MAX_QUANTITY_PER_ORDER);
    if qty.value() > max {
        return Err(OrderError::QuantityExceedsLimit(qty));
    }
    if today_pnl < -max_loss {
        return Err(OrderError::DailyLossLimitExceeded {
            today_pnl,
            max_loss,
        });
    }
    Ok(())
}

/// Deterministic platform order ID: `OMS-<client correlation uuid>`.
#[must_use]
pub fn platform_order_id(client_order_uuid: Uuid) -> String {
    format!("OMS-{client_order_uuid}")
}

/// Core order-acknowledgement construction (formerly the private
/// `_build_order_ack_inner`).
///
/// The acknowledged status is always [`OrderStatus::PendingNew`]: submission
/// registers the order as locally pending; the broker acknowledgement is a
/// separate lifecycle event.
///
/// # Errors
/// Propagates [`validate_order_request`] rejections.
pub fn submit_order(request: &OrderRequest, now: DateTime<Utc>) -> Result<OrderAck, OrderError> {
    validate_order_request(request)?;
    Ok(build_order_ack(request, now))
}

/// Build the deterministic acknowledgement for a validated request.
///
/// No clock and no UUID minting happen here: the order ID derives from the
/// request's correlation UUID and `now` is the caller-supplied timestamp.
#[must_use]
pub fn build_order_ack(request: &OrderRequest, now: DateTime<Utc>) -> OrderAck {
    let client_order_uuid = request.client_order_id;
    OrderAck::new(
        platform_order_id(client_order_uuid),
        client_order_uuid,
        OrderStatus::PendingNew,
        now,
    )
}

/// Validate a fill report's sanity before reconciliation.
///
/// # Errors
/// - [`OrderError::InvalidParameters`] for a zero fill or negative commission.
pub fn validate_fill_report(
    fill_quantity: Quantity,
    commission: Money,
    _fees: Money,
) -> Result<(), OrderError> {
    if fill_quantity.value() <= Decimal::ZERO {
        return Err(OrderError::InvalidParameters);
    }
    if commission.amount < Decimal::ZERO {
        return Err(OrderError::InvalidParameters);
    }
    Ok(())
}
