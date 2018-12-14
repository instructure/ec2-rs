use chrono::prelude::*;
use failure::{Error, err_msg};
use shellexpand::tilde as TildeExpand;
use serde_json::{from_str as JsonFromStr, to_string as JsonToStr, Value as JsonValue};

use std::fs::{create_dir as CreateDir, File, metadata as GetFileMetadata, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Handles Caching the Data returned from the EC2 API.
pub struct Cache {
  path_to_cache: String,
  valid_cache: bool,
  potential_json_value: Option<JsonValue>,
}

impl Cache {
  /// Creates a new instance of the Cache.
  pub fn new(root_path: String, account_names: String, timeout_seconds: u64) -> Result<Self, Error> {
    let expanded_path = TildeExpand(&root_path).into_owned();
    let path = Path::new(&expanded_path);
    let parent = path.parent();
    if !parent.is_some() {
      return Err(err_msg("Can't find root path!"));
    }
    let parent = parent.unwrap();
    if !parent.exists() {
      CreateDir(parent.to_str().unwrap())?;
    }

    let mut potential_json_value = None;
    let final_path = path.join(account_names);

    let valid_cache = if let Ok(metadata) = GetFileMetadata(Path::new(&final_path)) {
      if let Ok(created_at) = metadata.created() {
        if let Ok(duration) = created_at.elapsed() {
          if duration.as_secs() < timeout_seconds {
            true
          } else {
            false
          }
        } else {
          false
        }
      } else {
        use std::process::Command;
        // Metadata.created() returns nothing on non-bsd linux's :(
        if let Ok(output) = Command::new("date")
          .arg("-R")
          .arg("-r")
          .arg(&final_path)
          .output()
        {
          if let Ok(as_str) = String::from_utf8(output.stdout) {
            if let Ok(dt) = DateTime::parse_from_rfc2822(as_str.trim()) {
              if (dt.timestamp() + timeout_seconds as i64) >= Utc::now().timestamp() {
                true
              } else {
                false
              }
            } else {
              false
            }
          } else {
            false
          }
        } else {
          false
        }
      }
    } else {
      false
    };

    if path.exists() && final_path.exists() {
      let mut non_optional_file = File::open(final_path.clone())?;

      let mut as_str = String::new();
      let result = non_optional_file.read_to_string(&mut as_str);
      if result.is_ok() {
        if as_str.len() != 0 {
          let data = JsonFromStr(&as_str);
          if data.is_ok() {
            let value = data.unwrap();
            potential_json_value = Some(value);
          }
        }
      }
    }

    Ok(Cache {
      path_to_cache: final_path.to_str().unwrap().to_owned(),
      potential_json_value: potential_json_value,
      valid_cache: valid_cache,
    })
  }

  /// Determines if the cache is valid based on the timeout the user set.
  pub fn is_cache_valid(&self) -> bool {
    self.valid_cache
  }

  /// Determines if the Cache has data regardless of whether or not it's valid.
  pub fn has_cache_data(&self) -> bool {
    self.potential_json_value.is_some()
  }

  /// Grabs the data out of the cache. Consuming the cache as it should no longer be needed.
  /// The cache will only be read when we need to respond with it.
  pub fn get_cache_data(self) -> Result<String, Error> {
    if self.potential_json_value.is_some() {
      let json_value = self.potential_json_value.unwrap();
      return Ok(JsonToStr(&json_value)?);
    }
    Err(err_msg("No cache data!"))
  }


  /// Writes the new cache data. Consuming the cache as it should no longer be needed.
  pub fn write_cache_data(self, to_write: &str) -> Result<(), Error> {
    let mut file = OpenOptions::new().create(true).write(true).open(
      self.path_to_cache,
    )?;
    file.write_all(to_write.as_bytes())?;
    Ok(())
  }
}
