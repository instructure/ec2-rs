#![recursion_limit="128"]

extern crate chrono;
extern crate concurrent_hashmap;
#[macro_use]
extern crate error_chain;
extern crate fnv;
#[macro_use]
extern crate lazy_static;
extern crate openssl_probe;
extern crate rayon;
extern crate regex;
extern crate rusoto_core;
extern crate rusoto_ec2;
extern crate rusoto_sts;
extern crate shellexpand;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_json;
extern crate slog_term;
extern crate toml;

pub mod cache;
pub mod config;
#[macro_use]
pub mod ec2_utils;
pub mod errors;
pub mod provide_shallow_credentials;
pub mod role_assumer;

use concurrent_hashmap::*;
use fnv::FnvHashMap;
use rayon::prelude::*;
use rusoto_core::{default_tls_client as GetRusotoTls, Region};
use rusoto_ec2::{DescribeInstancesRequest, Ec2, Ec2Client, Filter};
use serde_json::Value as JsonValue;
use shellexpand::tilde as TildeExpand;
use slog::Drain;

use std::env;
use std::fs::OpenOptions;
use std::iter::FromIterator;
use std::path::Path;
use std::str::FromStr;

use cache::Cache;
use ec2_utils::*;
use role_assumer::RoleAssumer;

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

  info!(logger, "Configured EC2-RS Logging.");
  let path = env::var("EC2_RS_PATH").unwrap_or(
    env::current_dir()
      .expect("Failed to read current dir")
      .to_str()
      .unwrap()
      .to_owned(),
  );
  let config_path = Path::new(&TildeExpand(&path).into_owned()).join("ec2-ini.toml");
  if !config_path.exists() {
    panic!("Failed to find configuration file! Please make sure you have an ec2-ini.toml!");
  }
  info!(logger, "Found a config!");
  let config = config::parse_configuration(&config_path).expect(
    "Failed to parse config file! Please make sure your config is valid!",
  );
  info!(logger, "Parsed a configuration!");
  let assume = RoleAssumer::new().expect("Failed to find connect to AWS!");
  let role_to_assume: Option<String>;

  if let Ok(roles) = env::var("EC2_RS_ASSUME_ROLE") {
    role_to_assume = Some(roles);
  } else {
    role_to_assume = None;
  }
  info!(logger, "Parsed the role to assume");

  let cache = Cache::new(
    config.get_cache_path(),
    role_to_assume.clone(),
    config.get_cache_max_age(),
  ).expect("Failed to setup cache!");
  info!(logger, "Read Cache");

  if env::var("EC2_RS_FORCE_CACHE").is_ok() {
    info!(logger, "Trying to force cache!");
    if cache.has_cache_data() {
      info!(logger, "Cache has cache data.");
      let finalized_data = cache.get_cache_data().expect("Failed to read from cache!");
      return println!("{}", finalized_data);
    }
    panic!("Failed to fetch cache data!");
  }

  info!(logger, "Checking cache validity: {} && {}", cache.is_cache_valid(), cache.has_cache_data());
  if cache.is_cache_valid() && cache.has_cache_data() {
    let finalized_data = cache.get_cache_data().expect("Failed to read from cache!");
    return println!("{}", finalized_data);
  }

  info!(logger, "Creating filters");
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
    info!(logger, "Preparing host mode filters.");
    let hosts_split: Vec<String> = hosts.split(',').map(|val| val.to_owned()).collect();
    to_filter.push(Filter {
      name: Some("instance-id".to_owned()),
      values: Some(hosts_split),
    });
  }
  let to_filter = if to_filter.len() == 0 {
    info!(logger, "Nothing to filter at all");
    None
  } else {
    info!(logger, "Has some filter data");
    Some(to_filter)
  };

  let initial_request = DescribeInstancesRequest {
    dry_run: Some(false),
    filters: to_filter.clone(),
    instance_ids: None,
    max_results: None,
    next_token: None,
  };

  info!(logger, "Compiling Regex!");
  let include_regex = regex::Regex::new(&config.ec2.get_include_filter()).expect("Failed to compile include regex!");
  let exclude_regex = regex::Regex::new(&config.ec2.get_exclude_filter()).expect("Failed to compile exclude regex!");
  info!(logger, "Compiled Regex!");

  let roles = role_to_assume.unwrap_or("__default".to_owned());
  let mut listed_roles: Vec<String> = roles.split(",").map(|val| val.to_owned()).collect();

  info!(logger, "Creating meta vars map");
  let meta_vars = ConcHashMap::<String, JsonValue>::new();
  info!(logger, "Created");

  let result: Vec<JsonValue> = listed_roles
    .par_iter_mut()
    .map(|account| {

      info!(logger, "Par iter entered for Account: {}", account);
      let creds = if account != "__default" {
        assume.assume_role(account.clone()).expect(
          "Failed to assume role!",
        )
      } else {
        assume.get_default_creds().expect(
          "Failed to get AWS Creds!",
        )
      };

      config
        .ec2
        .get_regions()
        .par_iter()
        .map(|region| {
          info!(logger, "[{}] Iterating over Region: {}", account, region);
          let ec2 = Ec2Client::new(
            GetRusotoTls().expect("Failed to get TLS Client."),
            creds.clone(),
            Region::from_str(region).expect("Failed to read region"),
          );
          info!(logger, "[{}-{}] Created EC2 Client", account, region);

          if env::var("EC2_HOSTS").is_ok() {
            info!(logger, "[{}-{}] Host mode entered", account, region);
            let mut the_results = Vec::with_capacity(25);
            info!(logger, "[{}-{}] Created results vec.", account, region);
            if let Ok(described_instances) = ec2.describe_instances(&initial_request) {
              info!(logger, "[{}-{}] Found Described Instances", account, region);
              if let Some(reservations) = described_instances.reservations {
                info!(logger, "[{}-{}] Found some reservations", account, region);
                for reservation in reservations {
                  if let Some(instances) = reservation.instances {
                    info!(logger, "[{}-{}] Found some instances for reservation", account, region);
                    for instance in instances {
                      info!(logger, "[{}-{}] Parsing host output", account, region);
                      the_results.push(format_for_host_output(&instance, &account));
                      info!(logger, "[{}-{}] Parsed.", account, region);
                    }
                  }
                }
              }
            }
            the_results
          } else {
            info!(logger, "[{}-{}] Entering Non-Host Mode", account, region);
            let mut the_results = Vec::with_capacity(250);
            info!(logger, "[{}-{}] Describing instances", account, region);
            if let Ok(described_instances) = ec2.describe_instances(&initial_request) {
              info!(logger, "[{}-{}] Described Instances", account, region);
              if let Some(reservations) = described_instances.reservations {
                info!(logger, "[{}-{}] Found reservations.", account, region);
                for reservation in reservations {
                  if let Some(instances) = reservation.instances {
                    info!(logger, "[{}-{}] Found Instances", account, region);
                    for mut instance in instances {
                      info!(logger, "[{}-{}] Checking if instance should be added.", account, region);
                      if !instance_should_be_added(&config, &mut instance) {
                        continue;
                      }

                      info!(logger, "[{}-{}] Should be added checking dest variable", account, region);
                      let dest_variable = get_instance_dest_variable(&config, &instance);
                      if dest_variable.is_none() {
                        continue;
                      }
                      let dest_variable = dest_variable.unwrap();
                      info!(logger, "[{}-{}] Got dest variable, checking regex.", account, region);

                      if !include_regex.is_match(&dest_variable) || exclude_regex.is_match(&dest_variable) {
                        continue;
                      }

                      info!(logger, "[{}-{}] Formatting for meta.", account, region);
                      meta_vars.insert(
                        dest_variable.clone(),
                        format_for_host_output(&instance, &account),
                      );
                      let mut map = FnvHashMap::with_capacity_and_hasher(10, Default::default());
                      info!(logger, "[{}-{}] Formatted for meta.", account, region);

                      info!(logger, "[{}-{}] Inserting into map.", account, region);
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
                      info!(logger, "[{}-{}] Inserting security groups.", account, region);
                      if let Some(sg_names) = get_raw_security_group_names(&instance) {
                        for isg in sg_names {
                          map.insert(
                            to_safe(&format!("security_group_{}", isg)),
                            json!(&dest_variable),
                          );
                        }
                      }
                      info!(logger, "[{}-{}] Inserted values, checking flag/tag tags.", account, region);
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
                      info!(logger, "[{}-{}] Inserting Dest Variable", account, region);
                      map.insert("ec2".to_owned(), json!(&dest_variable));

                      info!(logger, "[{}-{}] Pushing result into map", account, region);
                      the_results.push(json!(map));
                    }
                  }
                }
              }
            }
            the_results
          }
        })
        .collect::<Vec<Vec<JsonValue>>>()
        .into_iter()
        .fold(Vec::new(), |mut acc, mut values| {
          info!(logger, "Fold Called for account: {}", account);
          acc.append(&mut values);
          acc
        })

    })
    .collect::<Vec<Vec<JsonValue>>>()
    .into_iter()
    .fold(Vec::new(), |mut acc, mut values| {
      info!(logger, "Called final fold.");
      acc.append(&mut values);
      acc
    });

  if env::var("EC2_HOSTS").is_ok() {
    println!(
      "{}",
      serde_json::to_string(&result).expect("Failed to render host info as JSON!")
    );
  } else {
    info!(logger, "Getting Ready to merge values");
    let hostvars = FnvHashMap::from_iter(meta_vars.iter());
    info!(logger, "Turned metavars into fnvhashmap");
    let mut meta = FnvHashMap::default();
    meta.insert("hostvars", json!(hostvars));
    info!(logger, "Inserted host vars");
    let mut base: FnvHashMap<String, JsonValue> = FnvHashMap::default();
    base.insert("_meta".to_owned(), json!(meta));
    info!(logger, "Inserted _meta");
    base.extend(merge_ec2_results(result).into_iter());
    info!(logger, "Merged successfully");

    let merged: JsonValue = json!(base);
    info!(logger, "Parsed JSON!");

    let as_string = serde_json::to_string_pretty(&merged).expect("Failed to render ec2.py output as JSON!");
    info!(logger, "Turned to string pretty");
    let _ = cache.write_cache_data(&as_string);
    info!(logger, "Written to cache");
    println!("{}", as_string);
  }
}
