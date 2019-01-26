/// Walking example.

use naming::Naming;
use rsure::{
    Estimate,
    Result,
    node::{
        self,
        fs,
        load,
        HashUpdater,
        NodeWriter,
        Source,
        SureNode,
    },
};
use std::{
    path::Path,
};

fn main() -> Result<()> {
    rsure::log_init();

    let base = ".";

    let mut naming = Naming::new(".", "haha", "dat", true);

    let mut estimate = Estimate { files: 0, bytes: 0 };
    let tmp_name = {
        let mut nf = naming.new_temp(true)?;
        naming.add_cleanup(nf.name.clone());
        let src = fs::scan_fs(base)?
            .inspect(|node| {
                match node {
                    Ok(n @ SureNode::File { .. }) => {
                        if n.needs_hash() {
                            estimate.files += 1;
                            estimate.bytes += n.size();
                        }
                    }
                    _ => (),
                }
            });
        node::save_to(&mut nf.writer, src)?;
        nf.name
    };
    println!("name: {:?}", tmp_name);

    // Update the hashes.
    let loader = Loader { name: &tmp_name };
    let hu = HashUpdater::new(loader, &mut naming);
    let hm = hu.compute(base, &estimate)?;
    let nf = naming.new_temp(true)?;
    hm.merge(&mut NodeWriter::new(nf.writer)?)?;

    naming.rename_to_main(&nf.name)?;

    Ok(())
}

struct Loader<'a> {
    name: &'a Path,
}

impl<'a> Source for Loader<'a> {
    fn iter(&mut self) -> Result<Box<dyn Iterator<Item = Result<SureNode>> + Send>> {
        Ok(Box::new(load(self.name)?))
    }
}
