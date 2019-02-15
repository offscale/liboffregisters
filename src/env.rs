pub fn env_or<T>(key: &str, default_env: T) -> std::ffi::OsString
    where
        T: Into<std::ffi::OsString>,
{
    match std::env::var_os(key) {
        Some(val) => val,
        None => default_env.into(),
    }
}

pub fn get_tmpdir() -> String {
    let _td = std::env::temp_dir();
    let _td_cow = _td.to_string_lossy();
    return _td_cow.as_ref().to_owned();
}

#[cfg(test)]
mod tests {
    use super::env_or;
    use std::env;
    use std::sync::Mutex;
    use std::sync::Arc;
    use std::panic;

    const KEY: &'static str = "FOO";
    const VALUE: &'static str = "BAR";
    lazy_static! {
        static ref MUTEX: Arc<Mutex<u8>> = Arc::new(Mutex::new(0 as u8));
    }

    fn run_test<T>(test: T) -> ()
        where
            T: FnOnce() -> () + panic::UnwindSafe {

        let m = MUTEX.lock().unwrap();
        let result = panic::catch_unwind(|| {
            test()
        });
        drop(m);

        assert!(result.is_ok())
    }

    #[test]
    fn env_or_some() {
        run_test(|| {
            env::set_var(KEY, VALUE);
            assert_eq!(env_or(KEY, KEY), VALUE);
        })
    }

    #[test]
    fn env_or_default() {
        run_test(|| {
            if env::var(KEY).is_ok() {
                env::remove_var(KEY);
            }
            assert_eq!(env_or(KEY, KEY), KEY);
        })
    }
}
