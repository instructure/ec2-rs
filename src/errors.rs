use rusoto_core::{CredentialsError, TlsError};
use serde_json::Error as SerdeError;
use std::io as StdIo;
use toml::de::Error as TomlDeError;

error_chain!{

  foreign_links  {
    IoError(StdIo::Error);
    JsonError(SerdeError);
    RusotoCredentialsError(CredentialsError);
    RusotoTlsError(TlsError);
    TomlDeserializeError(TomlDeError);
  }

  errors {
    RootPathError {
      description("Please don't create configs at the root path.")
      display("Please don't create configs at the root path.")
    }

    NoCacheData {
      description("There is no data in the cache!")
      display("There is no data inside this cache!")
    }
  }

}
