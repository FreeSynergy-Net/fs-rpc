//! The keys of the settings the scaffold declares for fs-rpc's behaviour. fs-rpc only holds the NAMES (`fs.<area>.<name>`, ADR 0100);
//! the declaration (kind, default, words) is the owner's: the scaffold, in `fs-starter` (its own repo since 0.0.4; ADR 0100 item 3). fs-rpc has no dependency on
//! `fs-settings`: the program reads the value and hands it in (`AskPolicy::from`, `SingleOptions::new`).
//!
//! Pattern: constants (the names only; the declaration is the owner's).

/// `fs.rpc.single.ask`: a yes/no, default yes. Whether the person is asked first about a server that could not be ended
/// (`no-control`, `not-ending`).
pub const SINGLE_ASK_KEY: &str = "fs.rpc.single.ask";

/// `fs.mcp.enabled`: a yes/no, default NO. Whether the MCP entry of a program may be switched on at all (MCP is off by default;
/// the vault is never reachable through MCP, whatever this says).
pub const MCP_ENABLED_KEY: &str = "fs.mcp.enabled";
