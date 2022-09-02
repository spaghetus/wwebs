use std::{collections::HashMap, str::FromStr};

use url::Url;

/// An HTTP-like request structure.
#[derive(Clone)]
#[non_exhaustive]
pub struct Request {
	/// The "verb" of the request.
	/// The meaning should be as close to HTTP as possible.
	/// An empty string is equivalent to "GET".
	pub verb: String,
	/// The URL of the request.
	pub url: url::Url,
	/// The headers passed in the request.
	pub headers: HashMap<String, String>,
	/// The body of the request, if applicable.
	pub body: Vec<u8>,
}

impl Default for Request {
	fn default() -> Self {
		Self {
			verb: String::default(),
			url: Url::from_str("http://localhost/").unwrap(),
			headers: HashMap::default(),
			body: Vec::default(),
		}
	}
}
