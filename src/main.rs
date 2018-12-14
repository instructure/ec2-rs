#![recursion_limit="128"]

pub mod cache;
pub mod config;
#[macro_use]
pub mod ec2_utils;

use concurrent_hashmap::*;
use fnv::FnvHashMap;
use rayon::prelude::*;
use rusoto_core::{HttpClient, Region};
use rusoto_credential::AutoRefreshingProvider;
use rusoto_ec2::{DescribeInstancesRequest, Ec2, Ec2Client, Filter};
use rusoto_sts::{StsClient, StsAssumeRoleSessionCredentialsProvider};
use serde_json::{json, Value as JsonValue};
use shellexpand::tilde as TildeExpand;
use slog::*;

use std::env;
use std::fs::{File, OpenOptions};
use std::iter::FromIterator;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::cache::Cache;
use crate::ec2_utils::*;

// Lib export thinks "main" is dead code, but we need lib export for benchmarks.
#[allow(dead_code)]
fn main() {
  openssl_probe::init_ssl_cert_env_vars();

  let logger = if env::var("EC2_RS_LOG_TO_FILE").is_ok() {
    let log_path = "ec2_rs_log.log";
    let file = OpenOptions::new()
      .create(true)
      .write(true)
      .truncate(true)
      .open(log_path)
      .expect("Failed to create open options.");

    let drain = slog_json::Json::new(file).add_default_keys().build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    slog::Logger::root(drain, o!())
  } else if env::var("EC2_RS_LOG_TO_CONSOLE").is_ok() {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    slog::Logger::root(drain, o!())
  } else {
    let drain = slog::Discard;
    slog::Logger::root(
      drain,
      o!()
    )
  };

  let path = env::var("EC2_RS_PATH").unwrap_or(
    env::current_dir()
      .expect("Failed to read current dir")
      .to_str()
      .unwrap()
      .to_owned(),
  );
  let config_path = Path::new(&TildeExpand(&path).into_owned()).join("ec2-ini.toml");
  if !config_path.exists() {
    panic!("Failed to find configuration file! Please make sure you have an ec2-ini.toml in your local dir, or EC2_RS_PATH!");
  }
  let config = config::parse_configuration(&config_path).expect(
    "Failed to parse config file! Please make sure your config is valid!",
  );
  let role_to_assume: String = env::var("EC2_RS_ASSUME_ROLE").expect("Assuming a role is needed!");

  let cache = Cache::new(
    config.get_cache_path(),
    role_to_assume.clone(),
    config.get_cache_max_age(),
  ).expect("Failed to setup cache!");

  if env::var("EC2_RS_FORCE_CACHE").is_ok() {
    if cache.has_cache_data() {
      let finalized_data = cache.get_cache_data().expect("Failed to read from cache!");
      return println!("{}", finalized_data);
    }
    panic!("Failed to fetch cache data!");
  }

  if cache.is_cache_valid() && cache.has_cache_data() {
    info!(logger, "Found valid cache!");
    let finalized_data = cache.get_cache_data().expect("Failed to read from cache!");
    return println!("{}", finalized_data);
  }

  let mut to_filter: Vec<Filter> = config
    .ec2
    .get_instance_filters()
    .into_iter()
    .map(|(key, value)| {
      Filter {
        name: Some(key.to_owned()),
        values: Some(
          value
            .to_owned()
            .split(",")
            .map(|val| val.to_owned())
            .collect(),
        ),
      }
    })
    .collect();

  if let Ok(hosts) = env::var("EC2_HOSTS") {
    let hosts_split: Vec<String> = hosts.split(',').map(|val| val.to_owned()).collect();
    to_filter.push(Filter {
      name: Some("instance-id".to_owned()),
      values: Some(hosts_split),
    });
  }
  let to_filter = if to_filter.len() == 0 {
    None
  } else {
    Some(to_filter)
  };

  let initial_request = DescribeInstancesRequest {
    dry_run: Some(false),
    filters: to_filter.clone(),
    instance_ids: None,
    max_results: None,
    next_token: None,
  };

  let include_regex = regex::Regex::new(&config.ec2.get_include_filter()).expect("Failed to compile include regex!");
  let exclude_regex = regex::Regex::new(&config.ec2.get_exclude_filter()).expect("Failed to compile exclude regex!");

  let mut listed_roles: Vec<String> = role_to_assume.split(",").map(|val| val.to_owned()).collect();

  let meta_vars = ConcHashMap::<String, JsonValue>::new();

  let mut role_assumption_mapping = FnvHashMap::default();
  let file_path = TildeExpand("~/.rapture/aliases.json").into_owned();
  let aliases_path = Path::new(&file_path);

  // If the path exists.
  if aliases_path.exists() {
    // And we can open the file.
    if let Ok(fd) = File::open(aliases_path) {
      // And we can parse it as json.
      if let Ok(json) = serde_json::from_reader(fd) {
        // Arbitrary Type-Ascription is not fully supported, so
        // give it some clue with a expression the compiler will
        // optimize away anyway.
        {
          let _a: &JsonValue = &json;
        }
        // And the root item is an object.
        if let Some(json_obj) = json.as_object() {
          // For each key.
          for (key, val) in json_obj.into_iter() {
            // If we have a string to string mapping...
            if val.is_string() {
              // Insert it as a role mapping.
              role_assumption_mapping.insert(key.to_owned(), val.as_str().unwrap().to_owned());
            }
          }
        }
      }
    }
  }

  let result: Vec<JsonValue> = listed_roles
    .par_iter_mut()
    .map(|account| {

      let sts = StsClient::new(Region::UsEast1);
      let creds = if role_assumption_mapping.contains_key(account) {
        Arc::new(AutoRefreshingProvider::new(
          StsAssumeRoleSessionCredentialsProvider::new(
            sts,
            role_assumption_mapping.get(account).unwrap().to_owned(),
            "default".to_owned(),
            None, None, None, None
          )
        ).expect("Failed to setup refreshing creds provider!"))
      } else {
        Arc::new(AutoRefreshingProvider::new(
          StsAssumeRoleSessionCredentialsProvider::new(
            sts,
            account.to_owned(),
            "default".to_owned(),
            None, None, None, None
          )
        ).expect("Failed to setup refreshing creds provider!"))
      };

      config
        .ec2
        .get_regions()
        .par_iter()
        .map(|region| {
          info!(logger, "[{}] Parsing region: {}", account, region);
          let ec2 = Ec2Client::new_with(
            HttpClient::new().expect("Failed to create HTTP(s) client!"),
            creds.clone(),
            Region::from_str(region).expect("Failed to read region"),
          );

          if env::var("EC2_HOSTS").is_ok() {
            let mut the_results = Vec::with_capacity(25);
            if let Ok(described_instances) = ec2.describe_instances(initial_request.clone()).with_timeout(Duration::from_secs(300)).sync() {
              if let Some(reservations) = described_instances.reservations {
                for reservation in reservations {
                  if let Some(instances) = reservation.instances {
                    for instance in instances {
                      the_results.push(format_for_host_output(&instance, &account));
                    }
                  }
                }
              }
            }
            the_results
          } else {
            let mut the_results = Vec::with_capacity(250);
            if let Ok(described_instances) = ec2.describe_instances(initial_request.clone()).with_timeout(Duration::from_secs(300)).sync() {
              if let Some(reservations) = described_instances.reservations {
                for reservation in reservations {
                  if let Some(instances) = reservation.instances {
                    for mut instance in instances {
                      if !instance_should_be_added(&config, &mut instance) {
                        continue;
                      }

                      let dest_variable = get_instance_dest_variable(&config, &instance);
                      if dest_variable.is_none() {
                        continue;
                      }
                      let dest_variable = dest_variable.unwrap();

                      if !include_regex.is_match(&dest_variable) || exclude_regex.is_match(&dest_variable) {
                        continue;
                      }

                      meta_vars.insert(
                        dest_variable.clone(),
                        format_for_host_output(&instance, &account),
                      );
                      let mut map = FnvHashMap::with_capacity_and_hasher(10, Default::default());

                      if let Some(iinstance_id) = instance.instance_id.clone() {
                        map.insert(iinstance_id, json!(&dest_variable));
                      }
                      if let Some(iregion) = get_raw_region_of_instance(&instance) {
                        map.insert(iregion, json!(&dest_variable));
                      }
                      if let Some(iplacement) = instance.placement.clone() {
                        if let Some(az) = iplacement.availability_zone {
                          map.insert(az, json!(&dest_variable));
                        }
                      }
                      if let Some(itype) = instance.instance_type.clone() {
                        map.insert(to_safe(&format!("type_{}", itype)), json!(&dest_variable));
                      }
                      if let Some(key_pair) = instance.key_name.clone() {
                        map.insert(to_safe(&format!("key_{}", key_pair)), json!(&dest_variable));
                      }
                      if let Some(ivpc_id) = instance.vpc_id.clone() {
                        map.insert(
                          to_safe(&format!("vpc_id_{}", ivpc_id)),
                          json!(&dest_variable),
                        );
                      }
                      if let Some(sg_names) = get_raw_security_group_names(&instance) {
                        for isg in sg_names {
                          map.insert(
                            to_safe(&format!("security_group_{}", isg)),
                            json!(&dest_variable),
                          );
                        }
                      }
                      instance.tags.as_ref().and_then(|tags| {
                        for tag in tags {
                          let tag_key = tag.key.as_ref().unwrap();
                          if tag_key == "Flags" {
                            let cloned_value = tag.value.as_ref().unwrap();
                            for icv in cloned_value.to_owned().split(",") {
                              map.insert(to_safe(&format!("flag_{}", icv)), json!(&dest_variable));
                            }
                          }
                          let itagkey = to_safe(&format!("tag_{}={}", tag_key, tag.value.as_ref().unwrap())
                            .to_lowercase());
                          map.insert(itagkey, json!(&dest_variable));
                        }
                        Some(())
                      });
                      map.insert("ec2".to_owned(), json!(&dest_variable));

                      the_results.push(json!(map));
                    }
                  }
                }
              }
            } else {
              panic!("Failed to describe instances!");
            }
            the_results
          }
        })
        .collect::<Vec<Vec<JsonValue>>>()
        .into_iter()
        .fold(Vec::new(), |mut acc, mut values| {
          acc.append(&mut values);
          acc
        })

    })
    .collect::<Vec<Vec<JsonValue>>>()
    .into_iter()
    .fold(Vec::new(), |mut acc, mut values| {
      acc.append(&mut values);
      acc
    });

  if env::var("EC2_HOSTS").is_ok() {
    println!(
      "{}",
      serde_json::to_string(&result).expect("Failed to render host info as JSON!")
    );
  } else {
    let hostvars = FnvHashMap::from_iter(meta_vars.iter());
    let mut meta = FnvHashMap::default();
    meta.insert("hostvars", json!(hostvars));
    let mut base: FnvHashMap<String, JsonValue> = FnvHashMap::default();
    base.insert("_meta".to_owned(), json!(meta));
    base.extend(merge_ec2_results(result).into_iter());

    let merged: JsonValue = json!(base);

    let as_string = serde_json::to_string(&merged).expect("Failed to render ec2.py output as JSON!");
    let _ = cache.write_cache_data(&as_string);
    println!("{}", as_string);
  }
}
