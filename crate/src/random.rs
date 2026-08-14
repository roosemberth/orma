use std::io::Read;

pub fn draw(count: usize) -> std::io::Result<Vec<u8>> {
    let mut drawn = vec![0u8; count];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut drawn)?;
    Ok(drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_draws_as_much_as_it_was_asked_for() {
        assert_eq!(draw(16).unwrap().len(), 16);
    }

    #[test]
    fn two_draws_differ() {
        assert_ne!(draw(16).unwrap(), draw(16).unwrap());
    }
}
