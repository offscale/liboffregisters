pub fn basename(path: &str) -> String {
    match path.rsplit(std::path::MAIN_SEPARATOR).next() {
        Some(p) => p.to_string(),
        None => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename() {
        assert_eq!(basename("foo/bar/can.txt"), "can.txt")
    }
}
