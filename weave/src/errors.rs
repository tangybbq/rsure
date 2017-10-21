// Errors in the weave code.

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Parse(::std::num::ParseIntError);
        Serde(::serde_json::Error);
    }
}
