use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use crate::{
	files::wwebs::WWebS,
	server::Server as WWebSServer,
	structures::{Request as WWebSRequest, Response as WWebSResponse},
	traits::Protocol,
};
use hyper::{
	body::Bytes,
	http::request,
	server::conn::Http,
	service::{make_service_fn, service_fn},
};
use hyper::{server::conn::AddrStream, service::Service};
use hyper::{Body, Request, Response, Server};
use std::net::TcpListener;
use url::Url;

pub struct HttpServer;

pub struct HttpConfig {
	pub ip: [u8; 4],
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
impl Protocol for HttpServer {
	type Request = WWebSRequest;
	type Response = WWebSResponse;

	/// Any relevant configuration for this server.
	type Config = HttpConfig;

	/// Starts the protocol.
	async fn run(&mut self, config: Self::Config, server: WWebSServer) -> anyhow::Result<()> {
		let addr = SocketAddr::from((config.ip, config.port));

		let make_svc = make_service_fn({
			|_conn: &AddrStream| {
				let server = server.clone();
				async move { Ok::<_, Infallible>(service_fn(move |r| Self::handle(server.clone(), r))) }
			}
		});

		let server;

		server = Server::bind(&addr).serve(make_svc);
		server.await?;
		Ok(())
	}
}

impl HttpServer {
	async fn handle(server: WWebSServer, r: Request<Body>) -> Result<Response<Body>, Infallible> {
		let mut request = WWebSRequest {
			verb: r.method().to_string(),
			url: {
				let uri = r.uri();
				let url = Url::parse(&uri.to_string())
					.unwrap_or_else(|_| Url::parse("http://localhost/").unwrap());
				url
			},
			headers: {
				r.headers()
					.iter()
					.map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
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
