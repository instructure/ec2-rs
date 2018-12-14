use failure::Error;
use shellexpand::tilde as TildeExpand;
use serde_derive::{Serialize, Deserialize};
use toml::from_str as parse_toml_string;

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Provides a Configuration Object for EC2-RS.
/// This is very similar to EC2.py in and of the sense everything is optional.
#[derive(Deserialize, Serialize)]
pub struct Configuration {
  /// The cache path to store our cache files at. Defaults to: `~/.ansible/tmp`.
  cache_path: Option<String>,
  /// The max age of the cache in seconds. Defaults to 300.
  cache_max_age: Option<u64>,
  /// The EC2 Configuration options.
  pub ec2: Ec2Configuration,
}

impl Configuration {
  /// Gets the Path to store our Cache Files at.
  pub fn get_cache_path(&self) -> String {
    self.cache_path.clone().unwrap_or(
      "~/.ansible/tmp".to_owned(),
    )
  }

  /// Gets the max age of the cache in seconds.
  pub fn get_cache_max_age(&self) -> u64 {
    self.cache_max_age.clone().unwrap_or(300)
  }
}

/// Provides all the configuration options for the EC2 scanning of ec2.py
#[derive(Deserialize, Serialize)]
pub struct Ec2Configuration {
  /// The Regions to scan. Defaults to us-{east,west}-{1,2},eu-{west,central}-1,ap-southeast-{1,2}, ca-central-1.
  regions: Option<Vec<String>>,
  /// Determines if you want non running instances or not.
  all_instances: Option<bool>,
  /// The destination variable, defaults to: `private_dns_name`.
  destination_variable: Option<String>,
  /// The destentation variable for things in a vpc, defaults to: `private_ip_address`.
  vpc_destination_variable: Option<String>,
  /// The Instance filters to use when scanning. Defaults to: "".
  instance_filters: Option<HashMap<String, String>>,
  /// An include pattern to only include hosts whose variable matches your regex.
  include_filter: Option<String>,
  /// An exclude pattern to exclude certain hosts whose variable matches your regex.
  exclude_filter: Option<String>,
}

impl Ec2Configuration {
  /// Get the regions to scan in for EC2.
  pub fn get_regions(&self) -> Vec<String> {
    self.regions.clone().unwrap_or(vec![
      "us-east-1".to_owned(),
      "us-east-2".to_owned(),
      "us-west-1".to_owned(),
      "us-west-2".to_owned(),
      "eu-west-1".to_owned(),
      "eu-central-1".to_owned(),
      "ap-southeast-1".to_owned(),
      "ap-southeast-2".to_owned(),
      "ca-central-1".to_owned(),
    ])
  }

  /// Gets the destination variable to write.
  pub fn get_dest_variable(&self) -> String {
    self.destination_variable.clone().unwrap_or(
      "private_dns_name"
        .to_owned(),
    )
  }

  /// Gets the destination variable for vpcs to write.
  pub fn get_vpc_dest_variable(&self) -> String {
    self.vpc_destination_variable.clone().unwrap_or(
      "private_ip_address"
        .to_owned(),
    )
  }

  /// Gets whether or not you want all instances.
  pub fn get_all_instances(&self) -> bool {
    self.all_instances.clone().unwrap_or(false)
  }

  /// Gets the instance filters to use.
  pub fn get_instance_filters(&self) -> HashMap<String, String> {
    self.instance_filters.clone().unwrap_or(HashMap::new())
  }

  pub fn get_include_filter(&self) -> String {
    self.include_filter.clone().unwrap_or(".*".to_owned())
  }

  pub fn get_exclude_filter(&self) -> String {
    self.exclude_filter.clone().unwrap_or("^$".to_owned())
  }
}

/// Parses a Configuration from a specified path.
pub fn parse_configuration(at_path: &PathBuf) -> Result<Configuration, Error> {
  let mut as_str = String::new();
  let mut file_handle = File::open(Path::new(
    &TildeExpand(at_path.to_str().unwrap()).into_owned(),
  ))?;
  file_handle.read_to_string(&mut as_str)?;
  Ok(parse_toml_string(&as_str)?)
}
