//! The backend for wwebs.

use std::{
	collections::HashMap,
	ffi::OsString,
	os::unix::prelude::PermissionsExt,
	path::{Path, PathBuf},
};

use subprocess::{Popen, PopenConfig};

use crate::{
	files::wwebs::WWebS,
	structures::{Request, Response},
};

/// The backend server for wwebs.
#[derive(Clone)]
pub struct Server {
	workdir: PathBuf,
}

impl Server {
	/// Creates the `DefaultBackend` with a given working directory.
	#[must_use]
	pub fn new(path: PathBuf) -> Server {
		Server { workdir: path }
	}

	/// Run a CGI binary. Don't call this on a static file, it won't go well.
	/// # Panics
	/// Panics if the path is empty.
	#[must_use]
	pub fn run_cgi(
		&self,
		request: &mut Request,
		path: &Path,
		config: &WWebS,
		query_strings: &HashMap<String, String>,
	) -> Response {
		// Make path relative
		let rel_path = path.strip_prefix(&self.workdir).unwrap();
		// Determine the path "inside" the target CGI binary
		let inside_path: PathBuf =
			if rel_path.components().count() < request.url.path_segments().unwrap().count() {
				// There is indeed a path inside the CGI binary
				let mut path_starts = rel_path.components().count();
				let last_component = rel_path
					.components()
					.last()
					.unwrap()
					.as_os_str()
					.to_string_lossy()
					.to_string();
				if [
					".gatekeeper",
					".req_transformer",
					".logger",
					".res_transformer",
				]
				.contains(&last_component.as_str())
				{
					path_starts -= 1;
				}

				request
					.url
					.path_segments()
					.unwrap()
					.skip(path_starts)
					.collect()
			} else {
				PathBuf::default()
			};
		let inside_path = inside_path.to_string_lossy().to_string();

		let p = Popen::create(
			&[path.to_string_lossy().to_string(), inside_path],
			PopenConfig {
				stdin: subprocess::Redirection::Pipe,
				stdout: subprocess::Redirection::Pipe,
				stderr: subprocess::Redirection::Pipe,
				cwd: Some(path.parent().unwrap().as_os_str().to_os_string()),
				env: Some({
					let mut env: Vec<(OsString, OsString)> = vec![];
					env.push(("PROTO".into(), request.proto.into()));
					for (k, v) in &request.headers {
						env.push((("HEADER_".to_string() + k).into(), v.into()));
					}
					for (k, v) in query_strings {
						env.push((("QUERY_".to_string() + k).into(), v.into()));
					}
					env.push(("VERB".into(), request.verb.clone().into()));
					env.push(("REQUESTED".into(), request.url.path().into()));
					for (k, v) in config.env.as_ref().unwrap_or(&HashMap::default()) {
						env.push((k.into(), v.clone().into()));
					}
					if let Ok(path) = std::env::var("PATH") {
						env.push(("PATH".into(), path.into()));
					}
					env
				}),
				..Default::default()
			},
		);

		if let Err(e) = p {
			eprintln!("{}", e);
			return Response::internal_server_error();
		}

		let mut p = p.unwrap();

		// Write the request body, and store the response.
		let (stdout, stderr) = match p.communicate_bytes(Some(&request.body)) {
			Ok((a, b)) => (a.unwrap_or_default(), b.unwrap_or_default()),
			Err(e) => {
				eprintln!("{:?}", e);
				return Response::internal_server_error();
			}
		};

		// Wait for p to exit...
		let exit_status = p.wait().unwrap_or(subprocess::ExitStatus::Exited(500));

		// Build the response.
		let mut response = Response {
			status: match exit_status {
				subprocess::ExitStatus::Exited(0) => 200,
				#[allow(clippy::cast_possible_truncation)]
				subprocess::ExitStatus::Exited(n) => n as u16,
				v => {
					eprintln!("{:?}", v);
					500
				}
			},
			headers: HashMap::default(),
			body: stdout,
		};

		// Parse the stderr...
		parse_output_commands(&stderr, &mut response);

		response
	}

	/// Execute a given path segment from a request.
	/// Recursively calls itself until we hit the final `run_cgi`.
	/// # Panics
	/// Panics when the url is a non-base url, which should never happen.
	#[must_use]
	pub fn exec(&self, request: &mut Request, segment: usize, config: &mut WWebS) -> Response {
		let path: PathBuf = request
			.url
			.path_segments()
			.expect("Unexpected cannot-be-a-base url")
			.take(segment)
			.map(|segment| {
				if let Some(percent_index) = segment.find('%') {
					&segment[..percent_index]
				} else {
					segment
				}
			})
			.collect();

		// Make the path absolute
		let path = self.workdir.join(path);
		let mut config = config.clone();

		// Get query strings
		let query_strings: HashMap<String, String> = request
			.url
			.query_pairs()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect();

		// Check that the path exists.
		if !path.exists() {
			return Response {
				status: 404,
				..Default::default()
			};
		}

		// Check that the path is allowed, and maybe executable.
		let (allowed, exec) = {
			let path = path.clone();
			let allowed_res: anyhow::Result<(bool, bool)> = (|| {
				let meta = std::fs::metadata(path)?;
				let permissions = meta.permissions();
				let mode = permissions.mode();
				Ok(((mode & 0o004) > 0, (mode & 0o001) > 0))
			})();
			allowed_res.unwrap_or((false, false))
		};
		if !allowed {
			return Response {
				status: 404,
				..Default::default()
			};
		}

		// Allocate the response
		let mut response: Response = Response::default();

		// Get the files in the directory
		let files: Vec<String> = get_files_at(&path);

		// If the path is a dir, perform all pre-request scoped operations.
		if path.is_dir() {
			// Extend config if possible
			Self::extend_config(&mut config, &path);
			// Evaluate all of the gatekeepers
			self.eval_gatekeepers(
				&files,
				&path,
				request,
				&config,
				&mut response,
				&query_strings,
			);
			// Execute all of the request transformers, but only if the response isn't already bad.
			if response.is_ok() {
				self.eval_req_transformers(&files, &path, request, &config, &query_strings);
			}
			// If the target is a directory and we are at the end, rewrite it to use the index.
			if response.is_ok()
				&& request.url.path_segments().unwrap().count() == segment
				&& path.is_dir()
			{
				let index = config
					.clone()
					.resolution
					.and_then(|v| v.index)
					.unwrap_or_else(|| "index.html".to_string());
				request.url.path_segments_mut().unwrap().push(&index);
			}
		}
		// Evaluate the target, but only if the request isn't already bad.
		if response.is_ok() {
			// Is the target a file?
			if path.is_file() {
				response = self.run_file(exec, &path, request, &config, &query_strings);
			} else {
				// The target is a directory, so we move into it.
				response = self.exec(request, segment + 1, &mut config);
			}
		}
		if path.is_dir() {
			self.eval_res_transformers(
				&files,
				&path,
				&config,
				&mut response,
				request,
				&query_strings,
			);
		}
		if response.status == 0 {
			response.status = 200;
		}
		// Run the loggers.
		self.run_loggers(&files, &path, &config, &response, request, &query_strings);
		response
	}

	fn run_loggers(
		&self,
		files: &[String],
		path: &Path,
		config: &WWebS,
		response: &Response,
		request: &mut Request,
		query_strings: &HashMap<String, String>,
	) {
		// Get the list of loggers.
		let mut loggers: Vec<&String> = files.iter().filter(|v| v.starts_with(".logger")).collect();
		loggers.sort();
		// Execute all of the response transformers.
		for logger in loggers {
			let path = path.join(logger);
			let mut extended_config = config.clone();
			extended_config
				.env
				.get_or_insert(HashMap::default())
				.insert("STATUS".to_string(), response.status.to_string());
			let _res = self.run_cgi(&mut request.clone(), &path, &extended_config, query_strings);
		}
	}

	fn eval_res_transformers(
		&self,
		files: &[String],
		path: &Path,
		config: &WWebS,
		response: &mut Response,
		request: &Request,
		query_strings: &HashMap<String, String>,
	) {
		// Get the list of response transformers.
		let mut res_transformers: Vec<&String> = files
			.iter()
			.filter(|v| v.starts_with(".res_transformer"))
			.collect();
		res_transformers.sort();
		// Execute all of the response transformers.
		for transformer in res_transformers {
			let path = path.join(transformer);
			let mut extended_config = config.clone();
			let env = extended_config.env.get_or_insert(HashMap::default());
			env.insert("STATUS".to_string(), response.status.to_string());
			let request = Request {
				proto: request.proto,
				verb: "GET".to_string(),
				url: request.url.clone(),
				headers: response.headers.clone(),
				body: response.body.clone(),
			};
			let res = self.run_cgi(&mut request.clone(), &path, &extended_config, query_strings);
			response.body = res.body;
			for (k, v) in res.headers {
				if v.is_empty() {
					response.headers.remove(&k);
				} else {
					response.headers.insert(k, v);
				}
			}
			response.status = res.status;
		}
	}

	fn run_file(
		&self,
		exec: bool,
		path: &Path,
		request: &mut Request,
		config: &WWebS,
		query_strings: &HashMap<String, String>,
	) -> Response {
		// Is the file static?
		match exec {
			false => {
				// Try to read the static file.
				if let Ok(data) = std::fs::read(path) {
					Response {
						status: 200,
						body: data,
						headers: HashMap::default(),
					}
				} else {
					Response {
						status: 500,
						..Default::default()
					}
				}
			}
			true => self.run_cgi(request, path, config, query_strings),
		}
	}

	fn eval_gatekeepers(
		&self,
		files: &[String],
		path: &Path,
		request: &Request,
		config: &WWebS,
		response: &mut Response,
		query_strings: &HashMap<String, String>,
	) {
		// Get the list of gatekeepers
		let mut gatekeepers: Vec<&String> = files
			.iter()
			.filter(|v| v.starts_with(".gatekeeper"))
			.collect();
		gatekeepers.sort();
		// Execute all of the gatekeepers.
		for gatekeeper in gatekeepers {
			let path = path.join(gatekeeper);
			let res = self.run_cgi(&mut request.clone(), &path, config, query_strings);
			if !res.is_ok() {
				*response = res;
			}
		}
	}

	fn eval_req_transformers(
		&self,
		files: &[String],
		path: &Path,
		request: &mut Request,
		config: &WWebS,
		query_strings: &HashMap<String, String>,
	) {
		// Get the list of request transformers.
		let mut transformers: Vec<&String> = files
			.iter()
			.filter(|v| v.starts_with(".req_transformer"))
			.collect();
		transformers.sort();
		// Execute all of the request transformers.
		for transformer in transformers {
			let path = path.join(transformer);
			let res = self.run_cgi(request, &path, config, query_strings);
			if res.is_ok() {
				for (k, v) in res.headers {
					if v.is_empty() {
						request.headers.remove(&k);
					} else {
						request.headers.insert(k, v);
					}
				}
				request.body = res.body;
			}
		}
	}

	fn extend_config(config: &mut WWebS, path: &Path) {
		*config = {
			let config_res: anyhow::Result<WWebS> = (|| {
				let config_path = path.join(".wwebs.toml");
				let config_string = std::fs::read_to_string(config_path)?;
				let config = toml::from_str(&config_string)?;
				Ok(config)
			})();
			config_res.unwrap_or_else(|_| config.clone())
		};
	}
}

fn get_files_at(path: &Path) -> Vec<String> {
	if path.is_dir() {
		let path = path;
		let files_res: anyhow::Result<_> = (|| {
			let readdir = std::fs::read_dir(path)?;
			Ok(readdir
				.flatten()
				.map(|v| v.file_name().to_string_lossy().to_string())
				.collect())
		})();
		files_res.unwrap_or_else(|_| vec![])
	} else {
		vec![]
	}
}

fn parse_output_commands(stderr: &[u8], response: &mut Response) {
	for line in String::from_utf8(stderr.to_vec())
		.unwrap_or_else(|_| String::default())
		.lines()
	{
		if line.starts_with("log ") {
			eprintln!("{}", line.strip_prefix("log ").unwrap_or("???"));
		} else if line.starts_with("header ") {
			let _res: Option<()> = (|| {
				let pair = line.strip_prefix("header ")?;
				let split = pair.find(' ')?;
				let key = &pair[..split];
				let value = &pair[1 + split..];
				response.headers.insert(key.to_string(), value.to_string());
				Some(())
			})();
		} else if line.starts_with("status ") {
			let status = line.strip_prefix("status ").unwrap().parse().unwrap_or(500);
			response.status = status;
		} else {
			eprintln!("{}", line);
		}
	}
}
