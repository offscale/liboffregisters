use std::fs::File;
use std::io::prelude::Write;
use std::path::Path;

use failure::Error;

pub fn write_string_to_file<T>(path: &Path, content: T) -> Result<(), Error>
where
    T: Into<String>,
{
    let display = path.display();
    let mut file = match File::create(&path) {
        Err(why) => Err(format_err!(
            "couldn't create {}: {:#?}",
            display,
            why.kind()
        )),
        Ok(file) => Ok(file),
    }?;
    match file.write_all(content.into().as_bytes()) {
        Err(why) => Err(format_err!(
            "couldn't write to {}: {:#?}",
            display,
            why.kind()
        )),
        Ok(_) => Ok(()),
    }
}

pub fn basename(path: &str) -> String {
    match path.rsplit(std::path::MAIN_SEPARATOR).next() {
        Some(p) => p.to_string(),
        None => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs::remove_file;
    use std::panic;
    use std::path::{Path, PathBuf};

    #[inline(always)]
    fn get_fname() -> PathBuf {
        Path::new(&*temp_dir()).join("test_write_file")
    }

    fn run_test<T>(test: T) -> ()
    where
        T: FnOnce() -> () + panic::UnwindSafe,
    {
        let cleanup = || {
            let fname = get_fname();
            if fname.exists() {
                remove_file(fname).unwrap();
            }
        };

        cleanup();

        let result = panic::catch_unwind(|| test());

        cleanup();

        assert!(result.is_ok())
    }

    #[test]
    fn test_basename() {
        assert_eq!(basename("foo/bar/can.txt"), "can.txt")
    }

    #[test]
    fn test_write_file() {
        run_test(|| {
            let fname = get_fname();

            let content: &'static str = "Foo";

            write_string_to_file(fname.as_path(), content).unwrap();
            assert_eq!(fname.exists(), true);
            let actual_content =
                std::fs::read_to_string(fname).expect("Something went wrong reading the file");
            assert_eq!(actual_content, content);
            assert_ne!(actual_content, "Not what I expected");
        })
    }
}
