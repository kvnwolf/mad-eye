//! The usage data core (headless): read Claude's OAuth usage endpoint, parse the
//! per-window utilization into Gauges, compute the Driving Gauge + Mood, and
//! shape the `Snapshot` the Popover renders.
//!
//! Public so the contract tests (`tests/usage_core.rs`) can reach the pure logic.

pub mod client;
pub mod mood;
pub mod types;
