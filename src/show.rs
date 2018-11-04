// Show module.

use crate::{suretree::SureTree, Result};
use std::path::Path;

pub fn show_tree(name: &Path) -> Result<()> {
    let tree = SureTree::load(name)?;
    println!("Nodes: {}", tree.count_nodes());
    // println!("{:#?}", tree);
    Ok(())
}
