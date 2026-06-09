pub(crate) const ROM_SIZE: usize = 32 * 1024;
pub(crate) const ENTRY_POINT: usize = 0x0100;

pub(crate) fn minimal_rom_with_entry_point(entry_point_bytes: &[u8]) -> Vec<u8> {
    let entry_point_end = ENTRY_POINT + entry_point_bytes.len();
    assert!(
        entry_point_end <= ROM_SIZE,
        "entry point bytes extend past the end of the ROM"
    );

    let mut rom = vec![0; ROM_SIZE];
    rom[ENTRY_POINT..entry_point_end].copy_from_slice(entry_point_bytes);
    rom
}
