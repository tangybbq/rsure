// Show module.

use std::path::Path;

use super::suretree::SureTree;
use super::Result;

pub fn show(name: &Path) -> Result<()> {
    let tree = try!(SureTree::load(name));
    println!("Nodes: {}", tree.count_nodes());
    // println!("{:#?}", tree);
    Ok(())
}
