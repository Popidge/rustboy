#![doc = "Core Game Boy emulation primitives."]

pub mod cartridge;

/// Placeholder type for the emulator core.
#[derive(Debug, Default)]
pub struct GameBoy;

impl GameBoy {
    /// Creates a new placeholder Game Boy instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::GameBoy;

    #[test]
    fn can_create_game_boy() {
        let game_boy = GameBoy::new();

        assert_eq!(format!("{game_boy:?}"), "GameBoy");
    }
}
