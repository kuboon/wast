/// Pattern types detected from wast bodies.
pub enum Pattern {
    /// loop + br_if with head condition → while
    While,
    /// loop + br_if with counter variable → for
    For,
    /// loop + br_if with list index → for-in
    ForIn,
    /// if (is_err) + return → try / ?
    Try,
}

/// Analyze a wast function body and detect high-level control flow patterns.
pub fn analyze(_body: &[u8]) -> Vec<Pattern> {
    todo!("implement pattern analysis")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_body() {
        let result = std::panic::catch_unwind(|| analyze(&[]));
        assert!(result.is_err(), "todo!() should panic");
    }
}
