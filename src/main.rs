use wwebs::{
	http::{Http, HttpConfig},
	server::Server,
	traits::Protocol,
};

fn main() {
	let workdir = std::env::current_dir().unwrap();
	let server = Server::new(workdir);

	let mut http = Http;
	let rt = tokio::runtime::Runtime::new().expect("failed to start runtime");
	let fut = http.run(
		HttpConfig {
			ip: [0, 0, 0, 0],
			port: 8000,
		},
		server,
	);
	rt.block_on(fut).expect("Failed");
}
