//! This module implements Gemini protocol support for wwebs.

use std::{collections::HashMap, io::Read};

use crate::{
	files::wwebs::WWebS,
	server::Server,
	structures::{Request, Response},
	traits::Protocol,
};
use async_trait::async_trait;
use openssl::hash::MessageDigest;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::TcpListener,
};
use tokio_native_tls::{native_tls::Identity, TlsAcceptor};
use url::Url;
use windmark::response::Response as WMResponse;

/// The marker struct for gemini servers.
pub struct Gemini;

#[async_trait]
impl Protocol for Gemini {
	type Request = GRequest;

	type Response = GResponse;

	type Config = GConfig;

	async fn run(self, config: Self::Config, server: Server) -> anyhow::Result<()> {
		windmark::router::Router::new()
			.set_private_key_file(config.private)
			.set_certificate_file(config.public)
			.mount("/*path", {
				let server = server.clone();
				move |ctx| {
					let url = ctx.url.clone();

					let req = GRequest {
						url,
						user_cert: ctx
							.certificate
							.and_then(|cert| cert.digest(MessageDigest::sha512()).ok())
							.map(base64::encode),
					};
					let mut req: Request = req.into();
					let response = server.exec(&mut req, 0, &mut WWebS::default());

					let response = GResponse {
						body: response.body.clone(),
						status: match response.status {
							200 => 20,
							500 => 50,
							v => v.try_into().unwrap_or(50),
						},
						meta: response
							.headers
							.get("X-GeminiMeta")
							.cloned()
							.unwrap_or_else(|| "text/gemini".to_owned()),
					};
					let meta = response.meta;
					let mut response = WMResponse::new(response.status, unsafe {
						String::from_utf8_unchecked(response.body)
					});
					response.with_mime(meta);
					response
				}
			})
			.set_error_handler(|_error| WMResponse::temporary_failure("Whoopsie"))
			.run()
			.await
			.expect("Gemini run failed");
		Ok(())
	}
}

/// The Gemini request structure.
pub struct GRequest {
	/// The URL of the request.
	pub url: Url,
	/// The user's certificate fingerprint, if they provided one.
	pub user_cert: Option<String>,
}

/// The Gemini response structure.
pub struct GResponse {
	/// The status code for the Gemini response.
	/// These status codes don't directly map to HTTP status codes, you should read the source code implementing `From<Response> for GResponse` to understand exactly how they're converted.
	pub status: i32,
	/// The metadata for the response.
	/// This is usually a MIME type.
	pub meta: String,
	/// The body of the response. This should be empty for non-2* responses.
	pub body: Vec<u8>,
}

/// The configuration struct used by the gemini protocol.
pub struct GConfig {
	/// The private key.
	pub private: String,
	/// The public key.
	pub public: String,
}

impl Default for GConfig {
	fn default() -> Self {
		Self {
			private: "./private.pem".to_string(),
			public: "public.pem".to_string(),
		}
	}
}

impl From<GRequest> for Request {
	fn from(req: GRequest) -> Self {
		Request {
			proto: "Gemini",
			verb: "GET".to_string(),
			url: {
				if req.url.has_authority() {
					req.url.clone()
				} else {
					let mut u = req.url.clone();
					u.set_host(Some("localhost")).unwrap();
					u.set_scheme("gemini").unwrap();
					u
				}
			},
			headers: {
				let mut h = HashMap::new();
				if let Some(c) = req.user_cert {
					h.insert("UserCert".to_string(), c);
				}
				if let Some(host) = req.url.host_str() {
					h.insert("Host".to_string(), host.to_string());
				}
				h
			},
			body: vec![],
		}
	}
}

impl From<Response> for GResponse {
	fn from(mut res: Response) -> Self {
		let status = match res.status {
			0 => 20,

			// It is assumed that valid gemini status codes are intentional and should be relayed
			#[allow(clippy::cast_possible_truncation)]
			n if n < 62 => n as u8,

			// All "OK" responses become 20.
			_ if res.is_ok() => 20,

			// Map redirect responses...
			301 => 31,
			302 => 30,
			// Catch-all for 300 responses, might cause issues
			n if (300..400).contains(&n) => 30,

			503 => 41,
			500 => 42,
			502 => 43,
			429 => 44,
			// Catch-all for 500 responses, assume they are temporary
			n if (500..600).contains(&n) => 40,

			404 => 51,
			410 => 52,
			400 => 59,

			401 => 60,
			403 => 61,

			// 600 codes aren't specified in http, so i'm going to use them for gemini responses that don't map well.
			// User input is requested.
			600 => 10,
			// Sensitive information is requested.
			601 => 11,
			_ => 42,
		};
		GResponse {
			status: status as i32,
			meta: {
				if res.is_ok() {
					if let Some(gemini_meta) = res.headers.remove("GEMINI_META") {
						gemini_meta
					} else if let Some(mime) = res.headers.get("Content-Type") {
						mime.to_string()
					} else {
						"application/octet-stream".to_string()
					}
				} else {
					match status {
						n if (30..40).contains(&n) => res
							.headers
							.get("Location")
							.map_or_else(|| "/".to_string(), std::clone::Clone::clone),
						n if (40..62).contains(&n) || (0..10).contains(&n) => {
							String::from_utf8(res.body.clone())
								.unwrap_or_else(|_| "???".to_string())
						}
						_ => "Body wasn't a string when it should have been".to_string(),
					}
				}
			},
			body: if res.is_ok() { res.body } else { vec![] },
		}
	}
}
