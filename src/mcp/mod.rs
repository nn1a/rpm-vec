#[cfg(feature = "mcp")]
pub mod protocol;
#[cfg(feature = "mcp")]
pub mod server;
#[cfg(feature = "mcp")]
pub mod tools;

#[cfg(feature = "mcp")]
pub use server::McpServer;
