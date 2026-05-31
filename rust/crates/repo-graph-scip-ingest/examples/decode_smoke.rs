//! Decoder de-risk runner (throwaway).
//!
//! Usage:
//!   cargo run -p repo-graph-scip-ingest --example decode_smoke -- <index.scip>
//!
//! Compare the printed counts against the spike's Node reader to confirm the
//! Rust `scip` decode path is correct.

use repo_graph_scip_ingest::{decode_index, summarize};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: decode_smoke <index.scip>");
    let bytes = std::fs::read(&path).expect("read scip file");
    let index = decode_index(&bytes).expect("decode scip index");
    println!("{:?}", summarize(&index));
}
