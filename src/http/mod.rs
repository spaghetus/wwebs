use std::{convert::Infallible, net::SocketAddr};

use crate::{
	files::wwebs::WWebS,
	server::Server as WWebSServer,
	structures::{Request as WWebSRequest, Response as WWebSResponse},
	traits::Protocol,
};
use cookie::Cookie;
use hyper::server::conn::AddrStream;
use hyper::{
	body::Bytes,
	service::{make_service_fn, service_fn},
};
use hyper::{Body, Request, Response, Server};
use url::Url;

/// The marker struct for the HTTP protocol implementation.
#[allow(clippy::module_name_repetitions)]
pub struct Http;

/// The configuration struct for the HTTP protocol implementation.
#[allow(clippy::module_name_repetitions)]
pub struct HttpConfig {
	/// The ipv4 address on which to listen.
	pub ip: [u8; 4],
	/// The TCP port on which to listen.
	pub port: u16,
}

impl Default for HttpConfig {
	fn default() -> Self {
		Self {
			ip: [127, 0, 0, 1],
			port: 8000,
		}
	}
}

#[async_trait::async_trait]
impl Protocol for Http {
	type Request = WWebSRequest;
	type Response = WWebSResponse;

	/// Any relevant configuration for this server.
	type Config = HttpConfig;

	/// Starts the protocol.
	async fn run(self, config: Self::Config, server: WWebSServer) -> anyhow::Result<()> {
		let addr = SocketAddr::from((config.ip, config.port));

		let make_svc = make_service_fn({
			|_conn: &AddrStream| {
				let server = server.clone();
				async move { Ok::<_, Infallible>(service_fn(move |r| Self::handle(server.clone(), r))) }
			}
		});

		let server = Server::bind(&addr).serve(make_svc);
		server.await?;
		Ok(())
	}
}

impl Http {
	async fn handle(server: WWebSServer, r: Request<Body>) -> Result<Response<Body>, Infallible> {
		let mut request = WWebSRequest {
			proto: "Http",
			verb: r.method().to_string(),
			url: {
				let http_uri = r.uri();
				let mut url = Url::parse("http://localhost/").unwrap();
				url.set_path(&http_uri.to_string());
				url
			},
			headers: {
				r.headers()
					.iter()
					.filter(|(k, _)| *k != "Cookie")
					.map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
					.chain(
						r.headers()
							.iter()
							.filter(|(k, _)| *k == "Cookie")
							.filter_map(|(_, v)| v.to_str().ok())
							.flat_map(|s| {
								s.split(';')
									.map(str::trim)
									.flat_map(Cookie::parse)
									.map(|cookie| {
										(
											format!("Cookie_{}", cookie.name().replace('-', "_")),
											cookie.value().to_string(),
										)
									})
									.collect::<Vec<_>>()
									.into_iter()
							}),
					)
					.collect()
			},
			body: hyper::body::to_bytes(r.into_body()).await.unwrap().to_vec(),
		};
		let response = server.exec(&mut request, 0, &mut WWebS::default());
		let mut hyper_res = Response::builder().status(response.status);
		for (k, v) in response.headers {
			hyper_res = hyper_res.header(k, v);
		}
		let bytes = Bytes::copy_from_slice(&response.body);
		let body = Body::from(bytes);
		let hyper_res = hyper_res.body(body);
		if let Ok(hyper_res) = hyper_res {
			Ok(hyper_res)
		} else {
			let (mut sender, body) = Body::channel();
			sender
				.send_data(Bytes::from("Whoopsie"))
				.await
				.expect("Failed to send");
			Ok(Response::builder().status(500).body(body).unwrap())
		}
	}
}
