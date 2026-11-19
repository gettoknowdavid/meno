//! Moderation: reports, the operator queue, and the block list (§2's tree).
//!
//! # Two features in one module
//!
//! They share a subject — "what do we do about an account or a broadcast" — and one
//! operator surface, which is why they land together:
//!
//! - **Reports** are what a listener files ([`handlers::create_report`]); the queue is an
//!   operator's ([`handlers::list_reports`], [`handlers::resolve_report`]).
//! - **Blocks** are a decision an account makes about another account
//!   ([`handlers::block`]), honoured by the modules that deliver things to it: `chat`
//!   refuses a message from somebody the host blocked, and `subscribers` refuses a follow
//!   of an account that blocked the follower.
//!
//! The split is the same five parts as every other module — `model`, `dto`, `error`,
//! `repository`, `service`, `handlers` — and [`state`] is the only file that names a
//! concrete adapter (§5.6).

pub mod dto;
pub mod error;
pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;
pub mod state;
