use std::fs;

fn main() {
    let path = "/path/to/.engram/stalks/test_scar_target.leg";
    if let Ok(data) = fs::read(path) {
        let mut block = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const engram_core::types::HolographicBlock) };
        block.crs_score = 0.85; // Set below genesis threshold
        let block_bytes = unsafe { std::slice::from_raw_parts(&block as *const _ as *const u8, std::mem::size_of::<engram_core::types::HolographicBlock>()) };
        fs::write(path, block_bytes).unwrap();
        println!("Adjusted CRS of test_scar_target to 0.85");
    } else {
        println!("Could not find test_scar_target at {}", path);
    }
}
