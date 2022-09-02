use std::collections::HashMap;

/// An HTTP-like representation of the server's response.
#[derive(Default, Clone, Debug)]
#[non_exhaustive]
pub struct Response {
	/// The status code of the response.
	/// The meaning of this field should be as close to HTTP as possible.
	/// Additionally, "0" means a successful response.
	pub status: u16,
	/// The headers of the response.
	/// Their meanings should be as close to HTTP as possible.
	pub headers: HashMap<String, String>,
	/// The body of the response.
	pub body: Vec<u8>,
}

impl Response {
	/// Returns whether the response is OK.
	#[must_use]
	pub fn is_ok(&self) -> bool {
		self.status == 0 || (200..300).contains(&self.status)
	}

	/// Helper to generate an HTTP 500 response.
	#[must_use]
	pub fn internal_server_error() -> Response {
		Response {
			status: 500,
			body: b"INTERNAL SERVER ERROR".to_vec(),
			..Default::default()
		}
	}
}
