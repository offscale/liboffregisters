use std::ffi::OsString;
use std::fs::create_dir_all;
use std::path::Path;

pub fn mkdirp<E>(path: E) -> Result<(), failure::Error>
where
    E: Into<OsString>,
{
    let p = path.into();
    if !Path::new(&p).exists() {
        create_dir_all(&p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename() {
        assert_eq!(Path::new("foo/bar/can.txt").file_name().unwrap(), "can.txt")
    }
}
