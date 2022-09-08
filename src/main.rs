use structopt::StructOpt;
use wwebs::{
	gemini::{GConfig, Gemini},
	http::{Http, HttpConfig},
	server::Server,
	traits::Protocol,
};

#[derive(structopt::StructOpt)]
struct Opts {
	/// The port to listen on for HTTP.
	#[structopt(short, long)]
	pub http_port: Option<u16>,
	/// The location of the Gemini private key.
	/// Make sure it isn't in the web directory and o+r, otherwise clients will be able to download it!!!
	/// Gemini will only be enabled if *both* options are set!!!
	#[structopt(short, long)]
	pub gem_cert: Option<String>,
	/// The password for decrypting the gemini identity.
	#[structopt(short = "G", long, env = "GEM_PASS")]
	pub gem_pass: Option<String>,
}

#[tokio::main]
async fn main() {
	let workdir = std::env::current_dir().unwrap();
	let server = Server::new(workdir);

	let opt = Opts::from_args();

	let http_fut = opt.http_port.map(|port| {
		tokio::task::spawn(Http.run(
			HttpConfig {
				ip: [0, 0, 0, 0],
				port,
			},
			server.clone(),
		))
	});

	let gem_fut = if let (Some(cert), Some(pass)) = (opt.gem_cert, opt.gem_pass) {
		Some(tokio::task::spawn(
			Gemini.run(GConfig { cert, pass }, server),
		))
	} else {
		None
	};
	if let Some(fut) = gem_fut {
		fut.await.unwrap().expect("Gemini failed");
	}
	if let Some(fut) = http_fut {
		fut.await.unwrap().expect("HTTP failed");
	}
}
