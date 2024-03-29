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
	pub gem_priv: Option<String>,
	/// The location of the Gemini public key.
	/// Gemini will only be enabled if *both* options are set!!!
	#[structopt(short = "G", long, env = "GEM_PASS")]
	pub gem_pub: Option<String>,
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

	let gem_fut = if let (Some(private), Some(public)) = (opt.gem_priv.clone(), opt.gem_pub.clone())
	{
		Some(tokio::task::spawn(
			Gemini.run(GConfig { private, public }, server),
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

	if let (None, None, None) = (opt.http_port, opt.gem_priv, opt.gem_pub) {
		eprintln!("You need to pass an http port or a Gemini certificate and password for wwebs to do anything");
	}
}
