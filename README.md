# EC2-RS #

EC2-RS is a replacement for the ec2.py script that is provided with ansible. With our current infastructure
of thousands of hosts it takes multiple minutes to go through one region at a time. As such ec2.py takes forever to run.
ec2.py also doesn't support assume role so for our multi aws account structure this doesn't work out the
easiest.

Thus enter EC2-RS, a replacement to ec2.py written in Rust covering all of Instructures current
use cases. Fixing everything that we could have ever complained about, or adding in everything
we've ever said "man I wish ec2.py could support X".

## Configuring EC2-RS ##

EC2-RS takes a toml configuration file, and a path that this configuration file is located at.
The easiest way to do this is copy the `example_config.toml` to the directory you want to keep
your config (maybe ~/.ansible)? and name it: `ec2-ini.toml`. (File names, and case sensitivty are important yo).

Once that's done simply configure the `EC2_RS_PATH` to be the directory of wherever you put that
configuration file. In order to make this easy simply put this in your `~/.bashrc` or `~/.bash_profile`.
This is much easier to have one global configuration rather than pasting an `ini` file around to every directory
you want to run ec2-rs from.

## Feature Compatibility with EC2.py ##

Feature Compatibility with EC2.py has been mostly dropped with v0.3, supporting
it was leading to some pains in maintenance. However, we're still compatible with
ansible itself! We just won't always export the exact same tags as EC2.py, or accept
the RDS/Eucalyptus config.

## Why is EC2-RS "Better"? ##

Some of the core features that make EC2-RS "better" than EC2.py for Instructure's uses are:

* EC2-RS is multi-threaded, meaning it can fetch from multiple regions at the same time.
* EC2-RS has the env var `EC2_RS_FORCE_CACHE` to better control caching regardless of general timeouts.
* EC2-RS uses roles to other accounts, and can fetch from multiple accounts.

These are the biggest reasons for EC2-RS, but you may find a couple more as you end up using it.

## So how do I build EC2-RS? ##


### Installing Rust ###


EC2-RS requires rust to build (obviously) just a normal binary locally, and `libssl-dev` (available through apt)/`openssl` in brew.

There are two ways to use Rust. Through Rustup (the rust version manager, and target manager),
or by manually manging rust versions.

There are a couple ways to install RustUp:
  - Through a bash script you can curl down (ala Brew): https://rustup.rs/ that downloads the correct binary for your platform.
  - Downloading Rustup-Init binary manually: https://github.com/rust-lang-nursery/rustup.rs/#other-installation-methods

Manually installing Rust versions can be done through standalone installers: [HERE](https://www.rust-lang.org/en-US/other-installers.html#standalone-installers)
The rust signing key is available: [HERE](https://static.rust-lang.org/rust-key.gpg.ascii), and also on keybase: [HERE](https://keybase.io/rust).

### Building ###

Simply run: `make` to build a normal non-static release version of the binary. If you'd like to build a debug version then you
can run: `make build`.

### Building Statically ###

Building Statically is currently possible, however it requires having the libmusl target for rust, as well
as having openssl compiled with libmusl in order to be built. I recommend taking a look at [rust-musl-builder][rust-musl-builder]
which is a docker image that already has everything you need setup.

From there you can just open up bash in the docker image, and run:

```bash
make build-static-release
```

## Okay, now how do I use EC2-RS? ##

Using EC2-RS is pretty simple once you've gotten it all setup (like having a toml file, and the program is built).

To assume a role you can simply pass a comma seperated list of accounts you'd like to mess with in an
environment variable. This can either be a full account arn to assume, or a name of an account in [rapture][rapture]. Like so:

```
EC2_RS_ASSUME_ROLE=account-one,account-two ansible-playbook -i ./my/path/to/ec2-rs/binary/ec2-rs --vault-password-file ~/.my-vault-pass playbooks/cool/playbook.yml
```

If you want to use the `--host` mode option from ec2.py instead of running ec2-rs with the `--host` command line flag
simply run ec2-rs with the env var `EC2_HOSTS` set to the comma seperated list of hosts you want info on. Like so:

```
EC2_HOSTS=i-123456789a,i-123456789a ./my/path/to/ec2-rs/binary/ec2-rs
```

Finally if you're running the same playbook over and over again (and your hosts are for sure not changing) you can
temporarily force the use of a cache with the `EC2_RS_FORCE_CACHE` env var, The mere presence of this will force the use of a cache.
In order for this to work, YOU MUST have a Cache present. It would probably look something like:

```
EC2_RS_FORCE_CACHE=1 EC2_RS_ASSUME_ROLE=account-one,account-two ansible-playbook -i ./my/path/to/ec2-rs/binary/ec2-rs --vault-password-file ~/.my-vault-pass playbooks/cool/playbook.yml
```

[rust-musl-builder]: https://github.com/emk/rust-musl-builder
[rapture]: https://github.com/daveadams/rapture
