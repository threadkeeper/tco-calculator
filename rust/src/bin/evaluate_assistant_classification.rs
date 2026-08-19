use azure_sql_tco::assistant::evaluation;

#[tokio::main]
async fn main() {
    if let Err(error) = evaluation::run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
