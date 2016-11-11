// Show module.

use std::path::Path;

use super::suretree::SureTree;
use super::Result;

pub fn show_tree(name: &Path) -> Result<()> {
    let tree = SureTree::load(name)?;
    println!("Nodes: {}", tree.count_nodes());
    // println!("{:#?}", tree);
    Ok(())
}
