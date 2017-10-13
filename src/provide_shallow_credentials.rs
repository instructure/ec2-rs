use rusoto_core::{AwsCredentials, CredentialsError, ProvideAwsCredentials};

/// Rusoto accepts only traits, but you cant say "Variable X is a trait"
/// because it's size isn't known at compile time, so we wrap it in this shallow
/// struct to help provide data.
#[derive(Clone, Debug)]
pub struct ProvideShallowCredentials {
  creds: AwsCredentials,
}

impl ProvideShallowCredentials {
  pub fn new(creds: AwsCredentials) -> Self {
    Self { creds: creds }
  }
}

impl ProvideAwsCredentials for ProvideShallowCredentials {
  fn credentials(&self) -> Result<AwsCredentials, CredentialsError> {
    Ok(self.creds.clone())
  }
}
