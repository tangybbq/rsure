// Show module.

use crate::{
    Store,
    Result,
    Version,
};

pub fn show_tree(store: &dyn Store) -> Result<()> {
    for node in store.load_iter(Version::Latest)? {
        let node = node?;
        println!("{:?}", node);
    }
    Ok(())
}
