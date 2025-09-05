use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, required = true)]
    pipeline: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    teisuu::config::builder(&args.pipeline).await
}
