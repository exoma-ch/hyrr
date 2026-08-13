//! MCP (Model Context Protocol) server for HYRR.
//!
//! When launched with `--mcp`, HYRR runs as a stdio MCP server
//! instead of the GUI. This enables AI assistants to invoke
//! nuclear physics calculations directly.

pub mod activity_at;
pub mod cache;
pub mod dataset;
pub mod dose;
pub mod nuclide;
pub mod tools;
pub mod transport;
pub mod viewer_export;
