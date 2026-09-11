//! Dumps the record tree of a .ppt PowerPoint Document stream (live or not)
//! to a given depth: offset, type, version, instance, length.

use pptxboss_core::cfb::Compound;
use pptxboss_core::zip::FileSource;

fn dump(data: &[u8], start: usize, end: usize, depth: usize, max_depth: usize) {
    let mut cursor = start;
    while cursor + 8 <= end {
        let word = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
        let ver = word & 0xf;
        let instance = word >> 4;
        let kind = u16::from_le_bytes([data[cursor + 2], data[cursor + 3]]);
        let len = u32::from_le_bytes([
            data[cursor + 4],
            data[cursor + 5],
            data[cursor + 6],
            data[cursor + 7],
        ]) as usize;
        println!(
            "{}{cursor:08x} type=0x{kind:04x} ver={ver} inst={instance} len={len}{}",
            "  ".repeat(depth),
            match cursor + 8 + len > end {
                true => "  <- OVERRUNS PARENT",
                false => "",
            }
        );
        if (ver == 0xf || kind == 0x040c) && depth < max_depth && cursor + 8 + len <= end {
            dump(data, cursor + 8, cursor + 8 + len, depth + 1, max_depth);
        }
        cursor += 8 + len;
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("path");
    let max_depth: usize = args.next().and_then(|d| d.parse().ok()).unwrap_or(2);
    let source = FileSource::open(&path).expect("opens");
    let compound = Compound::open(&source).expect("compound");
    let document = compound
        .stream("PowerPoint Document")
        .expect("document stream");
    println!("PowerPoint Document: {} bytes", document.len());
    dump(&document, 0, document.len(), 0, max_depth);
}
