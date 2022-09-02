use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The definition for the wwebs.toml file.
#[derive(Serialize, Deserialize, Clone, Default)]
#[non_exhaustive]
pub struct WWebS {
	/// The file resolution configuration, if any.
	pub resolution: Option<ResolutionInfo>,
	/// A hashmap of extra environment variables to set, if any.
	pub env: Option<HashMap<String, String>>,
}

impl std::ops::BitAnd for WWebS {
	type Output = WWebS;

	fn bitand(self, rhs: Self) -> Self::Output {
		WWebS {
			resolution: match (self.resolution, rhs.resolution) {
				(Some(v), None) | (None, Some(v)) => Some(v),
				(Some(a), Some(b)) => Some(a & b),
				(None, None) => None,
			},
			env: match (self.env, rhs.env) {
				(Some(v), None) | (None, Some(v)) => Some(v),
				(Some(a), Some(b)) => Some(a.into_iter().chain(b.into_iter()).collect()),
				(None, None) => None,
			},
		}
	}
}

/// Configuration for path resolution.
#[derive(Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct ResolutionInfo {
	/// Sets the name of the "index" file.
	pub index: Option<String>,
}

impl std::ops::BitAnd for ResolutionInfo {
	type Output = ResolutionInfo;

	fn bitand(self, rhs: Self) -> Self::Output {
		Self {
			index: match (self.index, rhs.index) {
				(None, None) => None,
				(_, Some(v)) | (Some(v), None) => Some(v),
			},
		}
	}
}
