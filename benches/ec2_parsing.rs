#![feature(test)]

extern crate ec2_rs_lib;
extern crate rusoto_ec2;
extern crate test;

use ec2_rs_lib::ec2_utils;
use rusoto_ec2::{Instance, InstanceState, GroupIdentifier, Placement, Tag};
use test::Bencher;

#[bench]
fn parse_ec2_structure(b: &mut Bencher) {
  let instance = Instance {
    ami_launch_index: Some(0),
    architecture: Some("x86_64".to_owned()),
    block_device_mappings: None,
    client_token: None,
    ebs_optimized: Some(false),
    elastic_gpu_associations: None,
    ena_support: None,
    hypervisor: Some("xen".to_owned()),
    iam_instance_profile: None,
    image_id: Some("ami-0a000a00".to_owned()),
    instance_id: Some("i-0a000a0aa0000aaaa".to_owned()),
    instance_lifecycle: None,
    instance_type: Some("m3.medium".to_owned()),
    kernel_id: None,
    key_name: Some("ops".to_owned()),
    launch_time: Some("2016-10-19T16:13:54.000Z".to_owned()),
    monitoring: None,
    network_interfaces: None,
    placement: Some(Placement {
      affinity: None,
      availability_zone: Some("us-east-1c".to_owned()),
      group_name: None,
      host_id: None,
      spread_domain: None,
      tenancy: None,
    }),
    platform: None,
    private_dns_name: None,
    private_ip_address: None,
    product_codes: None,
    public_dns_name: None,
    public_ip_address: None,
    ramdisk_id: None,
    root_device_name: Some("/dev/sda1".to_owned()),
    root_device_type: Some("ebs".to_owned()),
    security_groups: Some(vec![
      GroupIdentifier {
        group_id: Some("sg-aa000a00".to_owned()),
        group_name: Some("default".to_owned())
      },
      GroupIdentifier {
        group_id: Some("sg-000a000a".to_owned()),
        group_name: Some("web".to_owned())
      }
    ]),
    source_dest_check: Some(false),
    spot_instance_request_id: None,
    sriov_net_support: None,
    state: Some(InstanceState {
      code: Some(16),
      name: Some("running".to_owned()),
    }),
    state_reason: None,
    state_transition_reason: None,
    subnet_id: Some("subnet-aaa00aa0".to_owned()),
    tags: Some(vec![
      Tag {
        key: Some("Role".to_owned()),
        value: Some("Web".to_owned())
      }
    ]),
    virtualization_type: Some("hvm".to_owned()),
    vpc_id: Some("vpc-00a0000a".to_owned())
  };

  b.iter(|| {
    ec2_utils::format_for_host_output(&instance, "insops")
  });
}
