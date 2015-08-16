// Show module.

use ::suretree::SureTree;
use ::Result;

pub fn show(name: &str) -> Result<()> {
    let tree = try!(SureTree::load(name));
    println!("Nodes: {}", tree.count_nodes());
    // println!("{:#?}", tree);
    Ok(())
}
