mod common;

use common::{minimal_rom_with_entry_point, ENTRY_POINT, ROM_SIZE};

#[test]
fn creates_minimal_rom_with_entry_point_bytes() {
    let rom = minimal_rom_with_entry_point(&[0x00, 0x3E, 0x42]);

    assert_eq!(rom.len(), ROM_SIZE);
    assert_eq!(&rom[ENTRY_POINT..ENTRY_POINT + 3], &[0x00, 0x3E, 0x42]);
    assert_eq!(rom[ENTRY_POINT - 1], 0);
    assert_eq!(rom[ENTRY_POINT + 3], 0);
}

#[test]
#[should_panic(expected = "entry point bytes extend past the end of the ROM")]
fn rejects_entry_point_bytes_that_do_not_fit() {
    let oversized_entry_point = vec![0; ROM_SIZE - ENTRY_POINT + 1];

    let _rom = minimal_rom_with_entry_point(&oversized_entry_point);
}
