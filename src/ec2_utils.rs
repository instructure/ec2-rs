use config::Configuration;
use fnv::FnvHashMap;
use regex::Regex;
use rusoto_ec2::{Instance, Tag};
use serde_json::{Map as JsonMap, Value as JsonValue};

macro_rules! zoom_and_enchance {
  ($var:expr, $member:ident) => {
    ($var).$member.is_some()
  }
}

macro_rules! get_value_from_struct {
  ($var:expr, $member:ident) => {
    ($var).$member.clone().unwrap()
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

macro_rules! get_as_json_str {
  ($var:expr, $member:ident) => {
    {
      let value = ($var).$member.as_ref();
      if value.is_some() {
        json!(format!("{}", value.unwrap()))
      } else {
        json!("")
      }
    }
  };
}

macro_rules! json_map {
  { $($key:expr => $value:expr),+ } => {
    {
      let mut m = JsonMap::new();
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
      if !config.ec2.get_all_instances() && code != 16 {
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

fn get_potential_ec2_variable(var: &str, instance: &Instance) -> Option<String> {
  match var {
    "instance_id" => {
      if zoom_and_enchance!(instance, instance_id) {
        Some(get_value_from_struct!(instance, instance_id))
      } else {
        None
      }
    }
    "private_ip_address" => {
      if zoom_and_enchance!(instance, private_ip_address) {
        Some(get_value_from_struct!(instance, private_ip_address))
      } else {
        None
      }
    }
    "private_dns_name" => {
      if zoom_and_enchance!(instance, private_dns_name) {
        Some(get_value_from_struct!(instance, private_dns_name))
      } else {
        None
      }
    }
    "public_ip_address" => {
      if zoom_and_enchance!(instance, public_ip_address) {
        Some(get_value_from_struct!(instance, public_ip_address))
      } else {
        None
      }
    }
    _ => None,
  }
}

fn instance_is_monitored(instance: &Instance) -> bool {
  instance
    .monitoring
    .as_ref()
    .and_then(|data| {
      data
        .state
        .as_ref()
        .and_then(|state| Some(state == "enabled"))
        .or_else(|| Some(false))
    })
    .or_else(|| Some(false))
    .unwrap()
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
          .filter_map(|sgroup| sgroup.group_id.as_ref())
          .map(|frd_group| frd_group.to_owned())
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
          .filter_map(|sgroup| sgroup.group_name.as_ref())
          .map(|frd_group| frd_group.to_owned())
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

fn normalize_tag(tag: &Tag) -> (String, String) {
  let to_normalize = format!("ec2_tag_{}", tag.key.as_ref().unwrap());
  let normalized_key = SAFE_REGEX
    .replace_all(&to_normalize, "_")
    .into_owned()
    .to_owned()
    .to_lowercase();
  let value = tag.value.as_ref().unwrap().to_owned().to_lowercase();
  (normalized_key, value)
}

pub fn to_safe(string: &str) -> String {
  SAFE_REGEX.replace_all(string, "_").into_owned().to_owned()
}

pub fn get_instance_dest_variable(config: &Configuration, instance: &Instance) -> Option<String> {
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
    "ec2_ami_launch_index" =>  get_as_json_str!(instance, ami_launch_index),
    "ec2_architecture" => get_as_json!(instance, architecture),
    "ec2_client_token" => get_as_json!(instance, client_token),
    // ec2_dns_name is never set anywhere in ec2.py, and is never a non-empty string on an AWS Instance.
    // this only will ever happen in R53.
    "ec2_dns_name" => json!(""),
    "ec2_ebs_optimized" => get_as_json!(instance, ebs_optimized),
    // ec2_eventsSet is again a bug in ec2.py, and is never returned as a non empty string. EventsSet is
    // only ever returned when you describe an instance which ec2.py never does.
    "ec2_eventsSet" => json!(""),
    // ec2_group_name is also a bug in ec2.py, and is never returned as anything other than an empty
    // string. because group_name only belongs to security groups. however, ec2_security_group_names is populated.
    "ec2_group_name" => json!(""),
    "ec2_hypervisor" => get_as_json!(instance, hypervisor),
    "ec2_id" => get_as_json!(instance, instance_id),
    "ec2_image_id" => get_as_json!(instance, image_id),
    "ec2_instance_profile" => get_as_json!(instance, iam_instance_profile, arn),
    "ec2_instance_type" => get_as_json!(instance, instance_type),
    "ec2_ip_address" => get_as_json!(instance, public_ip_address),
    // Hey yo it's another empty string value. Probably only returned by eucalyptus or something.
    "ec2_item" => json!(""),
    "ec2_kernel" => get_as_json!(instance, kernel_id),
    "ec2_key_name" => get_as_json!(instance, key_name),
    "ec2_launch_time" => get_as_json!(instance, launch_time),
    "ec2_monitored" => json!(instance_is_monitored(instance)),
    // Surprise, Surprise another thing that isn't returned by this api call. Only monitoring state
    // is.
    "ec2_monitoring" => json!(""),
    "ec2_monitoring_state" => get_as_json!(instance, monitoring, state),
    // Do I really have to keep typing, or do you get the point by now?
    "ec2_persistent" => json!(false),
    "ec2_placement" => get_as_json!(instance, placement, availability_zone),
    "ec2_platform" => get_as_json!(instance, platform),
    // Insert whitty comment here.
    "ec2_previous_state" => json!(""),
    // insert another whitty comment.
    "ec2_previous_state_code" => json!(0),
    "ec2_public_dns_name" => get_as_json!(instance, public_dns_name),
    "ec2_ramdisk" => get_as_json!(instance, ramdisk_id),
    // so many whitty comments.
    "ec2_reason" => json!(""),
    "ec2_region" => get_region_of_instance(instance),
    // All the wit.
    "ec2_requester_id" => json!(""),
    "ec2_root_device_name" => get_as_json!(instance, root_device_name),
    "ec2_root_device_type" => get_as_json!(instance, root_device_type),
    "ec2_security_group_ids" => get_security_group_ids(instance),
    "ec2_security_group_names" => get_security_group_names(instance),
    "ec2_spot_instance_request_id" => get_as_json!(instance, spot_instance_request_id),
    "ec2_state" => get_as_json!(instance, state, name),
    "ec2_state_code" => get_as_json!(instance, state, code),
    "ec2_state_reason" => get_as_json!(instance, state_reason, message),
    "ec2_subnet_id" => get_as_json!(instance, subnet_id),
    "ec2_virtualization_type" => get_as_json!(instance, virtualization_type),
    "ec2_vpc_id" => get_as_json!(instance, vpc_id),
    "ec2_account_value" => json!(account),
    // Yes this needs to be a str
    "ec2_sourceDestCheck" => json!("false")
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
            let mut as_arr = current_value.as_array_mut().unwrap();
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
