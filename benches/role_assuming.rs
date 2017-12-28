#![feature(test)]

extern crate ec2_rs_lib;
extern crate rusoto_ec2;
extern crate shellexpand;
extern crate test;

use ec2_rs_lib::role_assumer;
use shellexpand::tilde as TildeExpand;
use test::Bencher;

use std::fs::{File, remove_file as RmFile};
use std::io::prelude::*;
use std::path::Path;

#[bench]
fn test_rapture_read(b: &mut Bencher) {
  let expanded_path = TildeExpand("~/.rapture/aliases.json").into_owned();
  let rapture_path = Path::new(&expanded_path);
  let created_path = if !rapture_path.exists() {
    println!("Created a path, if we error out you'll probably want to manually clear this.");
    let mut file = File::create(rapture_path.clone()).expect("Failed to create a file. Please create ~/.rapture/aliases.json");
    let _ = file.write_all(b"{\n\"account\": \"arn:aws:iam::000000000000:role/xacct/role\"\n}");
    true
  } else {
    false
  };

  b.iter(|| {
    role_assumer::RoleAssumer::new()
  });

  if created_path {
    let res = RmFile(rapture_path);
    if res.err().is_some() {
      println!("Failed to remove ~/.rapture/aliases.json");
    } else {
      println!("I removed the file I created.");
    }
  }
}
