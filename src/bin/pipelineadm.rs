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

    let vp = renzokutai::config::ValidatedPipeline::load(&args.pipeline)?.expect("Unknown pipeline");
    vp.run().await
}
