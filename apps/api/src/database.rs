pub async fn create_postgres_pool(url: &str) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await
        .expect("Failed to connect to PostgreSQL DB")
}
