use std::time::Duration;

pub async fn create_postgres_pool(url: &str) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .test_before_acquire(true)
        .connect(url)
        .await
        .expect("Failed to connect to PostgreSQL DB")
}
