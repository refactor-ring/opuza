use crate::common::*;

#[derive(PartialEq, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Config {
  paid: Option<bool>,
  pub(super) base_price: Option<Piconero>,
}

impl Config {
  pub(super) fn paid(&self) -> bool {
    self.paid.unwrap_or(false)
  }

  pub(super) fn for_dir(base_directory: &Path, path: &Path) -> Result<Self> {
    if !path.starts_with(base_directory) {
      return Err(Error::internal(format!(
        "Config::for_dir: `{}` does not start with `{}`",
        path.display(),
        base_directory.display()
      )));
    }
    path.read_dir().context(error::FilesystemIo { path })?;
    let mut config = Self::default();
    for path in path.ancestors() {
      if !path.starts_with(base_directory) {
        break;
      }
      let file_path = path.join(".opuza.yaml");
      match fs::read_to_string(&file_path) {
        Ok(yaml) => {
          let parent =
            serde_yaml::from_str(&yaml).context(error::ConfigDeserialize { path: file_path })?;
          config.merge_parent(parent);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => return Err(error::FilesystemIo { path: file_path }.into_error(source)),
      }
    }
    Ok(config)
  }

  fn merge_parent(&mut self, parent: Self) {
    *self = Self {
      paid: self.paid.or(parent.paid),
      base_price: self.base_price.or(parent.base_price),
    };
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use unindent::Unindent;

  #[test]
  fn test_default_config() {
    assert_eq!(
      Config {
        paid: None,
        base_price: None
      },
      Config::default()
    );
  }

  #[test]
  fn loads_the_default_config_when_no_files_given() {
    let temp_dir = TempDir::new().unwrap();
    let config = Config::for_dir(temp_dir.path(), temp_dir.path()).unwrap();
    assert_eq!(config, Config::default());
  }

  #[test]
  fn loads_config_from_files() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join(".opuza.yaml"), "paid: true").unwrap();
    let config = Config::for_dir(temp_dir.path(), temp_dir.path()).unwrap();
    assert_eq!(
      config,
      Config {
        paid: Some(true),
        base_price: None
      }
    );
  }

  #[test]
  fn directory_does_not_exist() {
    let temp_dir = TempDir::new().unwrap();
    let result = Config::for_dir(temp_dir.path(), &temp_dir.path().join("does-not-exist"));
    assert_matches!(
      result,
      Err(Error::FilesystemIo { path, source, .. })
        if path == temp_dir.path().join("does-not-exist") && source.kind() == io::ErrorKind::NotFound
    );
  }

  #[test]
  fn io_error_when_reading_config_file() {
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir(temp_dir.path().join(".opuza.yaml")).unwrap();
    let result = Config::for_dir(temp_dir.path(), temp_dir.path());
    assert_matches!(
      result,
      Err(Error::FilesystemIo { path, .. })
        if path == temp_dir.path().join(".opuza.yaml")
    );
  }

  #[test]
  fn invalid_config() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join(".opuza.yaml"), "{{{").unwrap();
    let result = Config::for_dir(temp_dir.path(), temp_dir.path());
    assert_matches!(
      result,
      Err(Error::ConfigDeserialize { path, .. })
        if path == temp_dir.path().join(".opuza.yaml")
    );
  }

  #[test]
  fn unknown_fields() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join(".opuza.yaml"), "unknown_field: foo").unwrap();
    let result = Config::for_dir(temp_dir.path(), temp_dir.path());
    assert_matches!(
      result,
      Err(Error::ConfigDeserialize { path, source, .. })
        if path == temp_dir.path().join(".opuza.yaml")
           && source.to_string().contains("unknown field `unknown_field`")
    );
  }

  #[test]
  fn paid_is_optional() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join(".opuza.yaml"), "{}").unwrap();
    let config = Config::for_dir(temp_dir.path(), temp_dir.path()).unwrap();
    assert_eq!(config, Config::default());
  }

  #[test]
  fn parses_base_price_in_xmr() {
    let temp_dir = TempDir::new().unwrap();
    let yaml = "
      paid: true
      base-price: 3 XMR
    "
    .unindent();
    fs::write(temp_dir.path().join(".opuza.yaml"), yaml).unwrap();
    let config = Config::for_dir(temp_dir.path(), temp_dir.path()).unwrap();
    assert_eq!(config.base_price, Some(Piconero::new(3_000_000_000_000)));
  }

  #[test]
  fn inherits_config() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
      temp_dir.path().join(".opuza.yaml"),
      "{paid: true, base-price: 1.5 XMR}",
    )
    .unwrap();
    fs::create_dir(temp_dir.path().join("dir")).unwrap();
    let config = Config::for_dir(temp_dir.path(), &temp_dir.path().join("dir")).unwrap();
    assert_eq!(
      config,
      Config {
        paid: Some(true),
        base_price: Some(Piconero::new(1_500_000_000_000))
      }
    );
  }

  #[test]
  fn override_paid() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
      temp_dir.path().join(".opuza.yaml"),
      "{paid: true, base-price: 1 XMR}",
    )
    .unwrap();
    fs::create_dir(temp_dir.path().join("dir")).unwrap();
    fs::write(temp_dir.path().join("dir/.opuza.yaml"), "paid: false").unwrap();
    let config = Config::for_dir(temp_dir.path(), &temp_dir.path().join("dir")).unwrap();
    assert_eq!(
      config,
      Config {
        paid: Some(false),
        base_price: Some(Piconero::ONE_XMR)
      }
    );
  }

  #[test]
  fn override_base_price() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
      temp_dir.path().join(".opuza.yaml"),
      "{paid: true, base-price: 3 XMR}",
    )
    .unwrap();
    fs::create_dir(temp_dir.path().join("dir")).unwrap();
    fs::write(temp_dir.path().join("dir/.opuza.yaml"), "base-price: 1 XMR").unwrap();
    let config = Config::for_dir(temp_dir.path(), &temp_dir.path().join("dir")).unwrap();
    assert_eq!(
      config,
      Config {
        paid: Some(true),
        base_price: Some(Piconero::ONE_XMR)
      }
    );
  }

  #[test]
  fn does_not_read_configs_in_subdirectories() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
      temp_dir.path().join(".opuza.yaml"),
      "{paid: true, base-price: 0.001 XMR}",
    )
    .unwrap();
    fs::create_dir(temp_dir.path().join("dir")).unwrap();
    fs::write(
      temp_dir.path().join("dir/.opuza.yaml"),
      "base-price: 0.002 XMR",
    )
    .unwrap();
    let config = Config::for_dir(temp_dir.path(), temp_dir.path()).unwrap();
    assert_eq!(
      config,
      Config {
        paid: Some(true),
        base_price: Some(Piconero::new(1_000_000_000))
      }
    );
  }

  #[test]
  fn does_not_read_configs_in_sibling_directories() {
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir(temp_dir.path().join("foo")).unwrap();
    fs::write(
      temp_dir.path().join("foo/.opuza.yaml"),
      "{paid: true, base-price: 1 XMR}",
    )
    .unwrap();
    fs::create_dir(temp_dir.path().join("bar")).unwrap();
    fs::write(
      temp_dir.path().join("bar/.opuza.yaml"),
      "{paid: true, base-price: 2 XMR}",
    )
    .unwrap();
    let config = Config::for_dir(temp_dir.path(), &temp_dir.path().join("foo")).unwrap();
    assert_eq!(
      config,
      Config {
        paid: Some(true),
        base_price: Some(Piconero::ONE_XMR)
      }
    );
  }

  #[test]
  fn does_not_read_configs_from_outside_the_root() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
      temp_dir.path().join(".opuza.yaml"),
      "{paid: true, base-price: 2 XMR}",
    )
    .unwrap();
    fs::create_dir(temp_dir.path().join("root")).unwrap();
    fs::create_dir(temp_dir.path().join("root/dir")).unwrap();
    let config =
      Config::for_dir(&temp_dir.path().join("root"), &temp_dir.path().join("root")).unwrap();
    assert_eq!(
      config,
      Config {
        paid: None,
        base_price: None
      }
    );
    let config = Config::for_dir(
      &temp_dir.path().join("root"),
      &temp_dir.path().join("root/dir"),
    )
    .unwrap();
    assert_eq!(
      config,
      Config {
        paid: None,
        base_price: None
      }
    );
  }
}
