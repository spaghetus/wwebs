//! This module implements Gemini protocol support for wwebs.

use std::collections::HashMap;

use crate::{
	files::wwebs::WWebS,
	server::Server,
	structures::{Request, Response},
	traits::Protocol,
};
use async_trait::async_trait;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::TcpListener,
};
use tokio_native_tls::{native_tls::Identity, TlsAcceptor};
use url::Url;

/// The marker struct for gemini servers.
pub struct Gemini;

#[async_trait]
impl Protocol for Gemini {
	type Request = GRequest;

	type Response = GResponse;

	type Config = GConfig;

	async fn run(self, config: Self::Config, server: Server) -> anyhow::Result<()> {
		let addr = "0.0.0.0:1965".to_string();
		let tcp: TcpListener = TcpListener::bind(&addr).await?;

		let der = std::fs::read(config.cert)?;
		let cert = Identity::from_pkcs12(&der, &config.pass)?;
		let tls_acceptor =
			TlsAcceptor::from(tokio_native_tls::native_tls::TlsAcceptor::builder(cert).build()?);

		// Begin listening
		loop {
			let server = server.clone();
			let (socket, remote_addr) = tcp.accept().await?;
			let tls_acceptor = tls_acceptor.clone();
			tokio::spawn(async move {
				let tls_stream = tls_acceptor.accept(socket).await;
				if let Err(e) = tls_stream {
					eprintln!("Bad request on gemini from {} because {}", remote_addr, e);
					return;
				}
				let mut tls_stream = tls_stream.unwrap();

				// Read URL
				let mut url = vec![];
				while let Ok(byte) = tls_stream.read_u8().await {
					if byte == b'\r' {
						break;
					}
					url.push(byte);
				}
				// Handle any errors with the url format
				let url = match String::from_utf8(url) {
					Err(_e) => {
						tls_stream
							.write_all(b"59 URL is not UTF8")
							.await
							.expect("Failed to write error response");
						return;
					}
					Ok(v) => match Url::parse(&v) {
						Err(_e) => {
							tls_stream
								.write_all(b"59 URL is not valid")
								.await
								.expect("Failed to write error response");
							return;
						}
						Ok(u) => u,
					},
				};

				// Get the client's certificate
				let user_cert = match tls_stream.get_ref().peer_certificate() {
					Err(_e) => {
						tls_stream
							.write_all(b"59 Invalid client cert")
							.await
							.expect("Failed to write error response");
						return;
					}
					Ok(c) => c.map(|v| base64::encode(v.to_der().unwrap_or_else(|_| vec![]))),
				};

				let gr = GRequest { url, user_cert };
				let res = server.exec(&mut gr.into(), 0, &mut WWebS::default());
				let g_res = GResponse::from(res);

				// Send response
				if let Err(e) = tls_stream
					.write_all(format!("{} {}\r\n", g_res.status, g_res.meta).as_bytes())
					.await
				{
					eprintln!("Failed to send header because {}", e);
				};

				// Send the body
				if let Err(e) = tls_stream.write_all(&g_res.body).await {
					eprintln!("Failed to send body because {}", e);
				};

				if let Err(e) = tls_stream.shutdown().await {
					eprintln!("Failed to close stream because {}", e);
				};
			});
		}
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
	pub status: u8,
	/// The metadata for the response.
	/// This is usually a MIME type.
	pub meta: String,
	/// The body of the response. This should be empty for non-2* responses.
	pub body: Vec<u8>,
}

/// The configuration struct used by the gemini protocol.
pub struct GConfig {
	/// The certificate path.
	pub cert: String,
	/// The certificate password.
	pub pass: String,
}

impl Default for GConfig {
	fn default() -> Self {
		Self {
			cert: "./identity.p12".to_string(),
			pass: "1234".to_string(),
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
			status,
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
