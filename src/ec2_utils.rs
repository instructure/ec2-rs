use crate::config::Configuration;

use fnv::FnvHashMap;
use lazy_static::lazy_static;
use regex::Regex;
use rusoto_ec2::{Instance, Tag};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

macro_rules! get_value_from_struct {
  ($var:expr, $member:ident) => {
    ($var).$member.as_ref()
  }
}

macro_rules! get_as_json {
  ($var:expr, $member:ident) => {
    {
      let value = ($var).$member.as_ref();
      if value.is_some() {
        json!(value.unwrap())
      } else {
        json!("")
      }
    }
  };
  ($var:expr, $member_one:ident, $member_two:ident) => {
    {
      let value = ($var).$member_one.as_ref();
      if value.is_some() {
        let internal_value = value.unwrap().$member_two.as_ref();
        if internal_value.is_some() {
          json!(internal_value.unwrap())
        } else {
          json!("")
        }
      } else {
        json!("")
      }
    }
  };
}

macro_rules! json_map {
  { $($key:expr => $value:expr),+ } => {
    {
      let mut m = JsonMap::with_capacity(19);
      $(
          m.insert($key.to_owned(), $value);
      )+
      m
    }
  };
  ($hm:ident, { $($key:expr => $value:expr),+ } ) => (
    {
      $(
          $hm.insert($key, $value);
      )+
    }
  );
}

lazy_static! {
  static ref SAFE_REGEX: Regex = Regex::new("[^A-Za-z0-9-]").unwrap();
}

pub fn instance_should_be_added(config: &Configuration, instance: &mut Instance) -> bool {
  if instance.state.is_none() {
    return false;
  }

  if let Some(ref mut state) = instance.state {
    if let Some(code) = state.code {
      if !config.ec2.get_all_instances() && (code & 0xff) != 16 {
        return false;
      }
    } else {
      return false;
    }
  } else {
    return false;
  }

  true
}

fn get_potential_ec2_variable<'a, 'b>(var: &'b str, instance: &'a Instance) -> Option<&'a String> {
  match var {
    "instance_id" => {
      get_value_from_struct!(instance, instance_id)
    }
    "private_ip_address" => {
      get_value_from_struct!(instance, private_ip_address)
    }
    "private_dns_name" => {
      get_value_from_struct!(instance, private_dns_name)
    }
    "public_ip_address" => {
      get_value_from_struct!(instance, public_ip_address)
    }
    _ => None,
  }
}

pub fn get_raw_region_of_instance(instance: &Instance) -> Option<String> {
  instance
    .placement
    .as_ref()
    .and_then(|placement| {
      placement
        .availability_zone
        .as_ref()
        .and_then(|az| {
          let location = az.rfind("-");

          if location.is_none() {
            None
          } else {
            let az_owned: String = az.to_owned();
            let (region, _) = az_owned.split_at(location.unwrap() + 2);
            Some(region.to_owned())
          }
        })
        .or_else(|| None)
    })
    .or_else(|| None)
}

fn get_region_of_instance(instance: &Instance) -> JsonValue {
  get_raw_region_of_instance(instance)
    .and_then(|data| Some(json!(data)))
    .or_else(|| Some(json!("")))
    .unwrap()
}

fn get_security_group_ids(instance: &Instance) -> JsonValue {
  instance
    .security_groups
    .as_ref()
    .and_then(|value| {
      Some(json!(
        value
          .iter()
          .filter_map(|sgroup| sgroup.group_id.clone())
          .collect::<Vec<String>>()
          .join(",")
      ))
    })
    .or_else(|| Some(json!("")))
    .unwrap()
}

pub fn get_raw_security_group_names(instance: &Instance) -> Option<Vec<String>> {
  instance
    .security_groups
    .as_ref()
    .and_then(|value| {
      Some(
        value
          .iter()
          .filter_map(|sgroup| sgroup.group_name.clone())
          .collect::<Vec<String>>(),
      )
    })
    .or_else(|| None)
}

fn get_security_group_names(instance: &Instance) -> JsonValue {
  get_raw_security_group_names(instance)
    .and_then(|data| Some(json!(data.join(","))))
    .or_else(|| Some(json!("")))
    .unwrap()
}

pub fn to_safe(string: &str) -> String {
  SAFE_REGEX.replace_all(string, "_").into_owned().to_owned()
}

fn normalize_tag(tag: &Tag) -> (String, String) {
  let to_normalize = format!("ec2_tag_{}", tag.key.as_ref().unwrap());
  let normalized_key = to_safe(&to_normalize).to_lowercase();
  let value = tag.value.clone().unwrap().to_lowercase();
  (normalized_key, value)
}

pub fn get_instance_dest_variable<'a, 'b>(config: &'b Configuration, instance: &'a Instance) -> Option<&'a String> {
  if instance.subnet_id.is_some() {
    get_potential_ec2_variable(&config.ec2.get_vpc_dest_variable(), instance)
  } else {
    get_potential_ec2_variable(&config.ec2.get_dest_variable(), instance)
  }
}

/// Formats an instance for Output of Host from EC2.py. Luckily for us
/// ec2.py only exports top level objects, and doesn't do crazy things like
/// map all block devices or something like that. So we can just get away
/// with some basically copy pasta'd kv values.
/// The full list is at the top of ec2.py.
pub fn format_for_host_output(instance: &Instance, account: &str) -> JsonValue {
  let mut map =
    json_map! {
    "ec2_account_value" => json!(account),
    "ec2_architecture" => get_as_json!(instance, architecture),
    "ec2_hypervisor" => get_as_json!(instance, hypervisor),
    "ec2_id" => get_as_json!(instance, instance_id),
    "ec2_image_id" => get_as_json!(instance, image_id),
    "ec2_instance_profile" => get_as_json!(instance, iam_instance_profile, arn),
    "ec2_instance_type" => get_as_json!(instance, instance_type),
    "ec2_ip_address" => get_as_json!(instance, public_ip_address),
    "ec2_key_name" => get_as_json!(instance, key_name),
    "ec2_placement" => get_as_json!(instance, placement, availability_zone),
    "ec2_region" => get_region_of_instance(instance),
    "ec2_root_device_name" => get_as_json!(instance, root_device_name),
    "ec2_root_device_type" => get_as_json!(instance, root_device_type),
    "ec2_security_group_ids" => get_security_group_ids(instance),
    "ec2_security_group_names" => get_security_group_names(instance),
    "ec2_state" => get_as_json!(instance, state, name),
    "ec2_subnet_id" => get_as_json!(instance, subnet_id),
    "ec2_virtualization_type" => get_as_json!(instance, virtualization_type),
    "ec2_vpc_id" => get_as_json!(instance, vpc_id)
  };

  let _ = instance.tags.as_ref().and_then(|tags: &Vec<Tag>| {
    for tag in tags {
      let (k, v) = normalize_tag(tag);
      map.insert(k, json!(v));
    }
    Some(())
  });

  JsonValue::Object(map)
}

pub fn merge_ec2_results(objects: Vec<JsonValue>) -> FnvHashMap<String, JsonValue> {
  let count = objects.len();
  objects.into_iter().fold(
    FnvHashMap::with_capacity_and_hasher(count, Default::default()),
    |mut acc: FnvHashMap<String, JsonValue>, mut value: JsonValue| {
      if !value.is_object() {
        return acc
      }
      let object = value.as_object_mut().unwrap();
      for (k, v) in object {
        if acc.contains_key(k) {
          let mut current_value = acc.remove(k).unwrap();

          if current_value.is_array() {
            let as_arr = current_value.as_array_mut().unwrap();
            as_arr.push(v.clone());

            acc.insert(k.to_owned(), json!(as_arr));
          }
        } else {
          let mut vec = Vec::with_capacity(5);
          vec.push(v);
          acc.insert(k.to_owned(), json!(vec));
        }
      }
      acc
    },
  )
}
