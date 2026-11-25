//! docfit: keeps a repo's documents each in its one home, within its budget.
//! `docfit --help` is the manual and its only copy.

mod budget;
mod cli;
mod documents;
mod kind;
mod repo;
mod sentences;

agent_help::binary!("docfit");

fn main() {
    let (answer, code) = cli::run(std::env::args_os());
    println!("{}", answer.trim_end());
    std::process::exit(code);
}
