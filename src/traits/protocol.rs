use crate::server::Server;

/// A trait for adding a new supported protocol to wwebs.
#[async_trait::async_trait]
pub trait Protocol {
	/// The request type that the server produces.
	/// It should translate as closely as possible to the HTTP-like Request type we provide.
	type Request: Into<crate::structures::Request>;
	/// The response type that the server uses.
	/// It should convey the meaning of the HTTP-like Response type we provide.
	type Response: From<crate::structures::Response>;

	/// Any relevant configuration for this server.
	type Config: Default;

	/// Starts the protocol.
	async fn run(self, config: Self::Config, server: Server) -> anyhow::Result<()>;
}
