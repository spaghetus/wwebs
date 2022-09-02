#![warn(missing_docs)]
#![warn(clippy::pedantic)]
// we stan match bool
#![allow(clippy::match_bool)]
//! wwebs is a cgi-first web server

/// Definitions for the configuration file syntax used by wwebs
pub mod files {
	/// Structures used in wwebs.toml.
	pub mod wwebs;
}

/// Trait definitions for using parts of wwebs or implementing your own protocol.
pub mod traits {
	mod protocol;
	pub use protocol::*;
}

/// Data structures used in wwebs.
pub mod structures {
	mod request;
	pub use request::*;
	mod response;
	pub use response::*;
}

pub mod server;

#[cfg(feature = "http")]
pub mod http;
