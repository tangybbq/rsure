// Show module.

use ::suretree::SureTree;
use ::Result;

pub fn show(name: &str) -> Result<()> {
    let tree = try!(SureTree::load(name));
    println!("{:#?}", tree);
    Ok(())
}
