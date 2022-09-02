//! The default backend for wwebs.

use std::{
	collections::HashMap,
	fs::File,
	os::unix::prelude::PermissionsExt,
	path::{Path, PathBuf},
};

use crate::{
	files::wwebs::WWebS,
	structures::{Request, Response},
};

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
	#[must_use]
	pub fn run_cgi(&self, request: &mut Request, path: &Path, config: &WWebS) -> Response {
		Response::default()
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
			.collect();

		// Make the path absolute
		let mut path = self.workdir.join(path);
		let mut config = config.clone();

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
		let files: Vec<String> = if path.is_dir() {
			let path = path.clone();
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
		};

		// If the path is a dir, perform all pre-request scoped operations.
		if path.is_dir() {
			// Extend config if possible
			Self::extend_config(&mut config, &path);
			// Evaluate all of the gatekeepers
			self.eval_gatekeepers(&files, &path, request, &mut config, &mut response);
			// Execute all of the request transformers, but only if the response isn't already bad.
			if response.is_ok() {
				self.eval_req_transformers(&files, &path, request, &mut config);
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
				response = self.run_file(exec, &path, request, &mut config);
			} else {
				// The target is a directory, so we move into it.
				response = self.exec(request, segment + 1, &mut config);
			}
		}
		if path.is_dir() {
			self.eval_res_transformers(&files, &path, &mut config, &mut response, request);
		}
		if response.status == 0 {
			response.status = 200;
		}
		response
	}

	fn eval_res_transformers(
		&self,
		files: &[String],
		path: &Path,
		config: &WWebS,
		response: &mut Response,
		request: &Request,
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
			extended_config
				.env
				.get_or_insert(HashMap::default())
				.insert("STATUS".to_string(), response.status.to_string());
			let res = self.run_cgi(&mut request.clone(), &path, &extended_config);
			if !res.is_ok() {
				*response = res;
			}
		}
	}

	fn run_file(&self, exec: bool, path: &Path, request: &mut Request, config: &WWebS) -> Response {
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
			true => self.run_cgi(request, path, config),
		}
	}

	fn eval_gatekeepers(
		&self,
		files: &[String],
		path: &Path,
		request: &Request,
		config: &WWebS,
		response: &mut Response,
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
			let res = self.run_cgi(&mut request.clone(), &path, config);
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
			let res = self.run_cgi(request, &path, config);
			if res.is_ok() {
				request.headers = res.headers;
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

impl Server {
	async fn process_request<RE: Into<Request> + Send, RS: From<Response> + Send>(
		&self,
		request: RE,
	) -> RS {
		let request: Request = request.into();

		todo!();
	}
}
