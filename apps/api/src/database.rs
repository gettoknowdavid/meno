use std::time::Duration;

/// Creates and configures a connection pool for a PostgreSQL database.
///
/// This function initializes a SQLx `PgPool` with the given connection settings
/// and connects to the database specified by the provided `url`.
///
/// # Parameters
/// - `url`: A string slice representing the database connection URL.
///
/// # Connection Pool Configuration
/// - `max_connections`: Sets the maximum number of connections in the pool to 20.
/// - `min_connections`: Sets the minimum number of connections retained in the pool to 2.
/// - `acquire_timeout`: Specifies the maximum duration (3 seconds) to wait for a connection
///   to be available before timing out.
/// - `idle_timeout`: Defines the duration (10 minutes) after which idle connections
///   in the pool are closed.
/// - `max_lifetime`: Establishes the maximum lifetime (30 minutes) of a connection
///   before it is closed and replaced.
/// - `test_before_acquire`: Ensures connections are verified before being handed
///   out from the pool, enhancing reliability.
///
/// # Returns
/// - A `sqlx::PgPool` instance representing the configured connection pool.
///
/// # Panics
/// - This function will panic if it fails to establish a connection to the database.
///
/// # Example
/// ```rust
/// use my_crate::create_postgres_pool;
/// use sqlx::PgPool;
///
/// #[tokio::main]
/// async fn main() {
///     let db_url = "postgres://user:password@localhost/database_name";
///     let pool: PgPool = create_postgres_pool(db_url).await;
///     // Use the database pool for queries
/// }
/// ```
pub async fn create_postgres_pool(url: &str) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_mins(10))
        .max_lifetime(Duration::from_mins(30))
        .test_before_acquire(true)
        .connect(url)
        .await
        .expect("Failed to connect to PostgreSQL DB")
}
