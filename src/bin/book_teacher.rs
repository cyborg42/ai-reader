use std::path::PathBuf;

use book_server::utils::init_log;
use clap::Parser;


#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(short, long)]
    book_path: PathBuf,
    #[clap(short, long)]
    student_id: i64,
}


#[tokio::main]
async fn main() ->  Result<(), Box<dyn std::error::Error>> {
    let _guard = init_log(None);
    let args = Args::parse();
    
    Ok(())
}