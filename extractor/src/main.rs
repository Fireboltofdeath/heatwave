use libmoonwave::{generate_docs_from_path, Args, Subcommand};
use std::env::current_dir;
use structopt::StructOpt;

async fn run(args: Args) -> anyhow::Result<()> {
    match args.subcommand {
        Subcommand::Extract(subcommand) => {
            let path = match subcommand.input_path {
                Some(path) => path,
                None => current_dir()?,
            };

            let base_path = match subcommand.base_path {
                Some(path) => path,
                None => path.clone(),
            };

            generate_docs_from_path(&path, &base_path).await
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::from_args();

    if let Err(error) = run(args).await {
        eprintln!("error: {}", error);
        std::process::exit(1);
    }
}
