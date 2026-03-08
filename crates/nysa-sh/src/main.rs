use nysa_core::app::App;
use sea_orm::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let username = "nysa";
    let password = "test-password";
    let host = "localhost";
    let port = 5432;
    let database = "nysa";

    let db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        username, password, host, port, database
    );
    let db = Database::connect(db_url).await?;

    let _app = App::init(db).await?;

    Ok(())
}
