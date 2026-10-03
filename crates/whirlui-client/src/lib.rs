//! The whirl protocol client.
//!
//! A frontend is an ordinary client of the daemon's protocol: it reads the
//! greeting, reads `status`, and follows `subscribe`, and it owns no state of
//! its own. The contract this crate obeys is whirl's `docs/architecture.md`
//! section 8, "Frontend contract", linked from the repository README.
