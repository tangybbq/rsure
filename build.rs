extern crate gcc;

fn main() {
    gcc::compile_library("librawlinux.a", &["c/dirent.c"]);
}
