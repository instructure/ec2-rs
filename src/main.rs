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

use std::env;
use std::iter::FromIterator;
use std::path::Path;
use std::str::FromStr;

use cache::Cache;
use ec2_utils::*;
use role_assumer::RoleAssumer;

fn main() {
  openssl_probe::init_ssl_cert_env_vars();
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
  let config = config::parse_configuration(&config_path).expect(
    "Failed to parse config file! Please make sure your config is valid!",
  );
  let assume = RoleAssumer::new().expect("Failed to find connect to AWS!");
  let role_to_assume: Option<String>;

  if let Ok(roles) = env::var("EC2_RS_ASSUME_ROLE") {
    role_to_assume = Some(roles);
  } else {
    role_to_assume = None;
  }

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

  let roles = role_to_assume.unwrap_or("__default".to_owned());
  let mut listed_roles: Vec<String> = roles.split(",").map(|val| val.to_owned()).collect();

  let meta_vars = ConcHashMap::<String, JsonValue>::new();

  let result: Vec<JsonValue> = listed_roles
    .par_iter_mut()
    .map(|account| {

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
          let ec2 = Ec2Client::new(
            GetRusotoTls().expect("Failed to get TLS Client."),
            creds.clone(),
            Region::from_str(region).expect("Failed to read region"),
          );

          if env::var("EC2_HOSTS").is_ok() {
            let mut the_results = Vec::new();
            if let Ok(described_instances) = ec2.describe_instances(&initial_request) {
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
            let mut the_results = Vec::new();
            if let Ok(described_instances) = ec2.describe_instances(&initial_request) {
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

    let as_string = serde_json::to_string_pretty(&merged).expect("Failed to render ec2.py output as JSON!");
    let _ = cache.write_cache_data(&as_string);
    println!("{}", as_string);
  }
}
