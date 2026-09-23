use crate::core::Result;

pub fn move_target(pinned: &[bool], from: usize, requested: usize) -> Result<usize> {
    if from >= pinned.len() || requested >= pinned.len() {
        return Err("This tab is no longer open.".into());
    }
    let boundary = pinned.iter().take_while(|pin| **pin).count();
    if pinned[boundary..].iter().any(|pin| *pin) {
        return Err("Pinned tabs must form the left-hand tab group.".into());
    }
    Ok(if pinned[from] {
        requested.min(boundary.saturating_sub(1))
    } else {
        requested.max(boundary)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tab_moves_cannot_cross_the_pin_boundary() {
        assert_eq!(move_target(&[true, true, false, false], 0, 3).unwrap(), 1);
        assert_eq!(move_target(&[true, true, false, false], 3, 0).unwrap(), 2);
        assert_eq!(move_target(&[false, false], 1, 0).unwrap(), 0);
        assert!(move_target(&[false, true], 0, 1).is_err());
        assert!(move_target(&[], 0, 0).is_err());
    }
}
