//! docfit: keeps a repo's documents each in its one home, within its budget.
//! `docfit --help` is the manual and its only copy.

mod budget;
mod cli;
mod documents;
mod kind;
mod paragraph;
mod pointers;
mod repo;

// build.rs runs it to stamp the version; compiled here too, for its tests.
#[cfg_attr(not(test), allow(dead_code))]
mod version;

agent_help::binary!("docfit");

fn main() {
    let (answer, code) = cli::run(std::env::args_os());
    println!("{}", answer.trim_end());
    std::process::exit(code);
}
