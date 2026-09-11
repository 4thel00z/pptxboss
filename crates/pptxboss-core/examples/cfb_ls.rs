//! Lists the streams of compound files with their sizes.

use pptxboss_core::cfb::Compound;
use pptxboss_core::zip::FileSource;

fn main() {
    for path in std::env::args().skip(1) {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let compound = FileSource::open(&path)
            .map_err(|e| e.to_string())
            .and_then(|source| Compound::open(&source).map_err(|e| e.to_string()));
        match compound {
            Ok(compound) => {
                let paths = compound.stream_paths();
                let sizes: Vec<String> = paths
                    .iter()
                    .map(|p| format!("{p}({})", compound.stream(p).map_or(0, |s| s.len())))
                    .collect();
                println!("{name}: {}", sizes.join(", "));
            }
            Err(err) => println!("{name}: ERROR {err}"),
        }
    }
}
