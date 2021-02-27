// Show module.

use crate::{Result, Store, Version};

pub fn show_tree(store: &dyn Store) -> Result<()> {
    for node in store.load_iter(Version::Latest)? {
        let node = node?;
        println!("{:?}", node);
    }
    Ok(())
}
